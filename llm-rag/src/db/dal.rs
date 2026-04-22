//! Server-side data access layer.
//!
//! `Dal` owns a single long-lived turso connection plus the active embedding
//! model's metadata. All writes that touch a vector are dimension-checked
//! against `self.model.dimensions` so a mismatched model can never silently
//! corrupt the per-dimension chunk table.
//!
//! Clients reach these operations over the Unix-socket RPC; the DAL itself is
//! never exposed across process boundaries.

use std::future::Future;
use std::path::Path;

use turso::{Builder, Connection, Value};

use super::error::DbError;
use super::migrations;
use super::schema::{self, EmbeddingModelInfo};

/// A stored document row. `created_at` and `updated_at` are ISO-8601 strings
/// populated server-side via `datetime('now')`.
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    pub id: i64,
    pub path: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Range of source content covered by a chunk. The enum discriminator maps
/// to the `range_kind` column in the chunk table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkRange {
    /// Inclusive page range.
    Pages { start: u32, end: u32 },
    /// Half-open byte range `[start, end)`.
    Bytes { start: u64, end: u64 },
}

impl ChunkRange {
    fn kind(&self) -> &'static str {
        match self {
            ChunkRange::Pages { .. } => "pages",
            ChunkRange::Bytes { .. } => "bytes",
        }
    }

    fn start_end_i64(&self) -> Result<(i64, i64), DbError> {
        match *self {
            ChunkRange::Pages { start, end } => Ok((start as i64, end as i64)),
            ChunkRange::Bytes { start, end } => {
                if start > i64::MAX as u64 || end > i64::MAX as u64 {
                    return Err(DbError::Query {
                        op: "chunk_range.encode",
                        source: turso::Error::ToSqlConversionFailure(
                            "byte offset exceeds i64 range".into(),
                        ),
                    });
                }
                Ok((start as i64, end as i64))
            }
        }
    }

    fn from_db(kind: &str, start: i64, end: i64) -> Result<Self, DbError> {
        match kind {
            "pages" => Ok(ChunkRange::Pages {
                start: start as u32,
                end: end as u32,
            }),
            "bytes" => Ok(ChunkRange::Bytes {
                start: start as u64,
                end: end as u64,
            }),
            other => Err(DbError::Query {
                op: "chunk_range.decode",
                source: turso::Error::ConversionFailure(format!("unexpected range_kind {other:?}")),
            }),
        }
    }
}

/// A single match returned from a vector search, ranked by cosine distance
/// (smaller is closer).
#[derive(Debug, Clone)]
pub struct ChunkMatch {
    pub chunk_id: i64,
    pub document_id: i64,
    pub content: String,
    pub range: ChunkRange,
    pub distance: f32,
}

/// A stored conversation header. `id` is a hyphenated UUID (minted by the
/// handler layer).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Conversation {
    pub id: String,
    pub title: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Role of a single row in `conversation_messages`. The on-wire / on-disk
/// string form is snake_case; enforced by [`MessageRole::from_db`] on read
/// and [`Dal::append_message`] on write since turso 0.4 has no CHECK support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    ToolUse,
    Tool,
}

impl MessageRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            MessageRole::System => "system",
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::ToolUse => "tool_use",
            MessageRole::Tool => "tool",
        }
    }

    pub fn from_db(s: &str) -> Result<Self, DbError> {
        match s {
            "system" => Ok(MessageRole::System),
            "user" => Ok(MessageRole::User),
            "assistant" => Ok(MessageRole::Assistant),
            "tool_use" => Ok(MessageRole::ToolUse),
            "tool" => Ok(MessageRole::Tool),
            other => Err(DbError::InvalidMessage {
                reason: format!("unknown role {other:?}"),
            }),
        }
    }
}

/// Strict, per-role metadata for a stored message. Serialized to JSON and
/// kept in the `metadata` TEXT column. `kind` disambiguates the variant so a
/// row's metadata is self-describing even if read without its `role`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MessageMetadata {
    ToolUse {
        tool_call_id: String,
        name: String,
        // Tool-call arguments are an open-ended JSON object defined by each
        // tool's schema; no stricter Rust type is meaningful here.
        arguments: serde_json::Value,
    },
    Tool {
        tool_call_id: String,
    },
}

impl MessageMetadata {
    fn matches_role(&self, role: MessageRole) -> bool {
        matches!(
            (self, role),
            (MessageMetadata::ToolUse { .. }, MessageRole::ToolUse)
                | (MessageMetadata::Tool { .. }, MessageRole::Tool)
        )
    }
}

/// One row in `conversation_messages`, ready for use by callers. `metadata`
/// is only populated for `ToolUse` / `Tool` rows.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StoredMessage {
    pub id: i64,
    pub conversation_id: String,
    pub role: MessageRole,
    pub content: String,
    pub metadata: Option<MessageMetadata>,
    pub created_at: String,
}

/// Filter passed to [`Dal::list_conversations`]. `tags` applies ALL-of
/// semantics — a conversation matches only when every listed tag is present.
#[derive(Debug, Clone, Default)]
pub struct ConversationFilter<'a> {
    pub tags: &'a [&'a str],
    pub text_query: Option<&'a str>,
    pub limit: Option<usize>,
}

/// Filter passed to [`Dal::list_documents_filtered`]. `tags` applies ALL-of
/// semantics — a document matches only when every listed tag is present.
#[derive(Debug, Clone, Default)]
pub struct DocumentFilter<'a> {
    pub tags: &'a [&'a str],
    pub limit: Option<usize>,
}

pub struct Dal {
    conn: Connection,
    model: EmbeddingModelInfo,
    chunk_table: String,
}

impl Dal {
    /// Open the DB, run migrations, resolve the embedding model's vector
    /// length (cache hit, or `probe` on miss), ensure the per-dimension chunk
    /// table exists, and return a ready DAL.
    ///
    /// `probe` is invoked at most once, only when no cached row exists for
    /// `model_name` — its result is then persisted to
    /// `embedding_model_dimensions` for subsequent boots.
    pub async fn open<F, Fut>(db_path: &Path, model_name: String, probe: F) -> Result<Self, DbError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<schema::EmbeddingProbe, DbError>>,
    {
        let path_str = db_path.to_string_lossy().to_string();
        let db = Builder::new_local(&path_str)
            .build()
            .await
            .map_err(|source| DbError::Open {
                path: db_path.to_path_buf(),
                source,
            })?;
        let conn = db.connect().map_err(|source| DbError::Open {
            path: db_path.to_path_buf(),
            source,
        })?;

        // Foreign keys are off by default in SQLite/turso; enable per-connection.
        conn.execute("PRAGMA foreign_keys = ON", ())
            .await
            .map_err(|source| DbError::Query {
                op: "pragma.foreign_keys",
                source,
            })?;

        migrations::run(&conn).await?;

        let (dimensions, max_input_tokens) =
            match schema::lookup_embedding_info(&conn, &model_name).await? {
                Some(cached) if cached.max_input_tokens.is_some() => {
                    tracing::info!(
                        model = %model_name,
                        dimensions = cached.dimensions,
                        max_input_tokens = ?cached.max_input_tokens,
                        "embedding model info resolved from cache",
                    );
                    (cached.dimensions, cached.max_input_tokens)
                }
                Some(cached) => {
                    // Row pre-dates migration 0004 — dimensions are trustworthy
                    // but max_input_tokens is NULL. Re-probe and backfill.
                    let probed = probe().await?;
                    if probed.dimensions != cached.dimensions {
                        tracing::warn!(
                            model = %model_name,
                            cached = cached.dimensions,
                            probed = probed.dimensions,
                            "probed dimensions differ from cached; keeping cached",
                        );
                    }
                    if let Some(tokens) = probed.max_input_tokens {
                        schema::update_max_input_tokens(&conn, &model_name, tokens).await?;
                    }
                    tracing::info!(
                        model = %model_name,
                        dimensions = cached.dimensions,
                        max_input_tokens = ?probed.max_input_tokens,
                        "embedding max_input_tokens backfilled from probe",
                    );
                    (cached.dimensions, probed.max_input_tokens)
                }
                None => {
                    let probed = probe().await?;
                    schema::record_embedding_info(&conn, &model_name, probed).await?;
                    tracing::info!(
                        model = %model_name,
                        dimensions = probed.dimensions,
                        max_input_tokens = ?probed.max_input_tokens,
                        "embedding model info probed and cached",
                    );
                    (probed.dimensions, probed.max_input_tokens)
                }
            };

        schema::ensure_chunk_table(&conn, dimensions).await?;

        let model = EmbeddingModelInfo {
            name: model_name,
            dimensions,
            max_input_tokens,
        };
        let chunk_table = schema::chunk_table_name(dimensions);
        Ok(Dal {
            conn,
            model,
            chunk_table,
        })
    }

    pub fn model(&self) -> &EmbeddingModelInfo {
        &self.model
    }

    // ---- Transactions -----------------------------------------------------
    //
    // turso 0.4 doesn't expose an async-friendly transaction/savepoint struct,
    // so callers compose BEGIN / COMMIT / ROLLBACK as plain statements. The
    // single shared connection means these are ordered strictly against any
    // other DAL call on the same `Dal`.

    pub async fn begin(&self) -> Result<(), DbError> {
        self.conn
            .execute("BEGIN", ())
            .await
            .map_err(|source| DbError::Query {
                op: "begin",
                source,
            })?;
        Ok(())
    }

    pub async fn commit(&self) -> Result<(), DbError> {
        self.conn
            .execute("COMMIT", ())
            .await
            .map_err(|source| DbError::Query {
                op: "commit",
                source,
            })?;
        Ok(())
    }

    pub async fn rollback(&self) -> Result<(), DbError> {
        self.conn
            .execute("ROLLBACK", ())
            .await
            .map_err(|source| DbError::Query {
                op: "rollback",
                source,
            })?;
        Ok(())
    }

    // ---- Documents --------------------------------------------------------

    pub async fn create_document(&self, path: &str) -> Result<i64, DbError> {
        self.conn
            .execute(
                "INSERT INTO documents (path) VALUES (?)",
                (path.to_string(),),
            )
            .await
            .map_err(|source| DbError::Query {
                op: "create_document",
                source,
            })?;
        Ok(self.conn.last_insert_rowid())
    }

    pub async fn get_document(&self, id: i64) -> Result<Option<Document>, DbError> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, path, created_at, updated_at FROM documents WHERE id = ?",
                (id,),
            )
            .await
            .map_err(|source| DbError::Query {
                op: "get_document",
                source,
            })?;
        match rows.next().await.map_err(|source| DbError::Query {
            op: "get_document.next",
            source,
        })? {
            Some(row) => Ok(Some(row_to_document(&row)?)),
            None => Ok(None),
        }
    }

    pub async fn get_document_by_path(&self, path: &str) -> Result<Option<Document>, DbError> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, path, created_at, updated_at FROM documents WHERE path = ?",
                (path.to_string(),),
            )
            .await
            .map_err(|source| DbError::Query {
                op: "get_document_by_path",
                source,
            })?;
        match rows.next().await.map_err(|source| DbError::Query {
            op: "get_document_by_path.next",
            source,
        })? {
            Some(row) => Ok(Some(row_to_document(&row)?)),
            None => Ok(None),
        }
    }

    pub async fn list_documents(&self) -> Result<Vec<Document>, DbError> {
        self.list_documents_filtered(&DocumentFilter::default())
            .await
    }

    pub async fn list_documents_filtered(
        &self,
        filter: &DocumentFilter<'_>,
    ) -> Result<Vec<Document>, DbError> {
        // ALL-of tag semantics via the same GROUP BY / HAVING pattern used by
        // `list_conversations`.
        let mut sql =
            String::from("SELECT d.id, d.path, d.created_at, d.updated_at FROM documents d");
        let mut params: Vec<Value> = Vec::new();

        if !filter.tags.is_empty() {
            sql.push_str(
                " INNER JOIN document_tags dt ON dt.document_id = d.id\
                 \n INNER JOIN tags t ON t.id = dt.tag_id",
            );
            let placeholders = vec!["?"; filter.tags.len()].join(", ");
            sql.push_str(&format!(" WHERE t.name IN ({placeholders})"));
            for tag in filter.tags {
                params.push(Value::Text((*tag).to_string()));
            }
        }

        sql.push_str(" GROUP BY d.id, d.path, d.created_at, d.updated_at");

        if !filter.tags.is_empty() {
            sql.push_str(" HAVING COUNT(DISTINCT t.id) = ?");
            params.push(Value::Integer(filter.tags.len() as i64));
        }

        sql.push_str(" ORDER BY d.id");

        if let Some(limit) = filter.limit {
            sql.push_str(" LIMIT ?");
            params.push(Value::Integer(limit as i64));
        }

        let mut rows = self
            .conn
            .query(&sql, turso::params::Params::Positional(params))
            .await
            .map_err(|source| DbError::Query {
                op: "list_documents_filtered",
                source,
            })?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await.map_err(|source| DbError::Query {
            op: "list_documents_filtered.next",
            source,
        })? {
            out.push(row_to_document(&row)?);
        }
        Ok(out)
    }

    pub async fn update_document_path(&self, id: i64, path: &str) -> Result<(), DbError> {
        let affected = self
            .conn
            .execute(
                "UPDATE documents SET path = ?, updated_at = datetime('now') WHERE id = ?",
                (path.to_string(), id),
            )
            .await
            .map_err(|source| DbError::Query {
                op: "update_document_path",
                source,
            })?;
        if affected == 0 {
            return Err(DbError::NotFound {
                kind: "document",
                id,
            });
        }
        Ok(())
    }

    pub async fn delete_document(&self, id: i64) -> Result<(), DbError> {
        let affected = self
            .conn
            .execute("DELETE FROM documents WHERE id = ?", (id,))
            .await
            .map_err(|source| DbError::Query {
                op: "delete_document",
                source,
            })?;
        if affected == 0 {
            return Err(DbError::NotFound {
                kind: "document",
                id,
            });
        }
        Ok(())
    }

    // ---- Tags -------------------------------------------------------------

    pub async fn add_tag(&self, document_id: i64, tag: &str) -> Result<(), DbError> {
        let tag_id = self.upsert_tag(tag).await?;
        self.conn
            .execute(
                "INSERT OR IGNORE INTO document_tags (document_id, tag_id) VALUES (?, ?)",
                (document_id, tag_id),
            )
            .await
            .map_err(|source| DbError::Query {
                op: "add_tag.link",
                source,
            })?;
        Ok(())
    }

    pub async fn remove_tag(&self, document_id: i64, tag: &str) -> Result<(), DbError> {
        // turso 0.4 doesn't support `IN (subquery)`, so we resolve tag_id in
        // a separate query. Missing tag name is a no-op (same as the union
        // semantics of the subquery form).
        let mut rows = self
            .conn
            .query("SELECT id FROM tags WHERE name = ?", (tag.to_string(),))
            .await
            .map_err(|source| DbError::Query {
                op: "remove_tag.select_tag",
                source,
            })?;
        let tag_id: Option<i64> = match rows.next().await.map_err(|source| DbError::Query {
            op: "remove_tag.select_tag.next",
            source,
        })? {
            Some(row) => Some(row.get(0).map_err(|source| DbError::Query {
                op: "remove_tag.select_tag.get",
                source,
            })?),
            None => None,
        };
        let Some(tag_id) = tag_id else {
            return Ok(());
        };
        self.conn
            .execute(
                "DELETE FROM document_tags WHERE document_id = ? AND tag_id = ?",
                (document_id, tag_id),
            )
            .await
            .map_err(|source| DbError::Query {
                op: "remove_tag.delete",
                source,
            })?;
        Ok(())
    }

    /// Replace the set of tags linked to `document_id` with exactly the
    /// supplied list. Duplicates in `tags` are deduped. Missing tag rows are
    /// created; orphaned rows in the global `tags` table are left alone
    /// (other documents may reference them).
    pub async fn set_tags(&self, document_id: i64, tags: &[&str]) -> Result<(), DbError> {
        // Upsert every tag first so all tag_ids exist.
        let mut tag_ids = Vec::with_capacity(tags.len());
        for tag in tags {
            tag_ids.push(self.upsert_tag(tag).await?);
        }

        self.conn
            .execute(
                "DELETE FROM document_tags WHERE document_id = ?",
                (document_id,),
            )
            .await
            .map_err(|source| DbError::Query {
                op: "set_tags.delete_old",
                source,
            })?;

        for tag_id in tag_ids {
            self.conn
                .execute(
                    "INSERT OR IGNORE INTO document_tags (document_id, tag_id) VALUES (?, ?)",
                    (document_id, tag_id),
                )
                .await
                .map_err(|source| DbError::Query {
                    op: "set_tags.insert",
                    source,
                })?;
        }
        Ok(())
    }

    async fn upsert_tag(&self, tag: &str) -> Result<i64, DbError> {
        self.conn
            .execute(
                "INSERT OR IGNORE INTO tags (name) VALUES (?)",
                (tag.to_string(),),
            )
            .await
            .map_err(|source| DbError::Query {
                op: "upsert_tag.insert",
                source,
            })?;
        let mut rows = self
            .conn
            .query("SELECT id FROM tags WHERE name = ?", (tag.to_string(),))
            .await
            .map_err(|source| DbError::Query {
                op: "upsert_tag.select",
                source,
            })?;
        let row = rows
            .next()
            .await
            .map_err(|source| DbError::Query {
                op: "upsert_tag.next",
                source,
            })?
            .ok_or(DbError::Query {
                op: "upsert_tag.empty",
                source: turso::Error::QueryReturnedNoRows,
            })?;
        row.get::<i64>(0).map_err(|source| DbError::Query {
            op: "upsert_tag.get",
            source,
        })
    }

    pub async fn tags_for_document(&self, document_id: i64) -> Result<Vec<String>, DbError> {
        let mut rows = self
            .conn
            .query(
                "SELECT t.name
                 FROM tags t
                 INNER JOIN document_tags dt ON dt.tag_id = t.id
                 WHERE dt.document_id = ?
                 ORDER BY t.name",
                (document_id,),
            )
            .await
            .map_err(|source| DbError::Query {
                op: "tags_for_document",
                source,
            })?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await.map_err(|source| DbError::Query {
            op: "tags_for_document.next",
            source,
        })? {
            let name: String = row.get(0).map_err(|source| DbError::Query {
                op: "tags_for_document.get",
                source,
            })?;
            out.push(name);
        }
        Ok(out)
    }

    pub async fn documents_with_tag(&self, tag: &str) -> Result<Vec<Document>, DbError> {
        let mut rows = self
            .conn
            .query(
                "SELECT d.id, d.path, d.created_at, d.updated_at
                 FROM documents d
                 INNER JOIN document_tags dt ON dt.document_id = d.id
                 INNER JOIN tags t           ON t.id = dt.tag_id
                 WHERE t.name = ?
                 ORDER BY d.id",
                (tag.to_string(),),
            )
            .await
            .map_err(|source| DbError::Query {
                op: "documents_with_tag",
                source,
            })?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await.map_err(|source| DbError::Query {
            op: "documents_with_tag.next",
            source,
        })? {
            out.push(row_to_document(&row)?);
        }
        Ok(out)
    }

    /// Tags starting with `prefix` (LIKE 'prefix%'), sorted alphabetically.
    /// `prefix` is matched literally — `%` and `_` in `prefix` are escaped so
    /// they don't behave as wildcards.
    pub async fn search_tags(&self, prefix: &str, limit: usize) -> Result<Vec<String>, DbError> {
        let escaped = prefix
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        let pattern = format!("{escaped}%");
        let mut rows = self
            .conn
            .query(
                "SELECT name FROM tags
                 WHERE name LIKE ? ESCAPE '\\'
                 ORDER BY name
                 LIMIT ?",
                (pattern, limit as i64),
            )
            .await
            .map_err(|source| DbError::Query {
                op: "search_tags",
                source,
            })?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await.map_err(|source| DbError::Query {
            op: "search_tags.next",
            source,
        })? {
            let name: String = row.get(0).map_err(|source| DbError::Query {
                op: "search_tags.get",
                source,
            })?;
            out.push(name);
        }
        Ok(out)
    }

    // ---- Chunks -----------------------------------------------------------

    pub async fn create_chunk(
        &self,
        document_id: i64,
        range: ChunkRange,
        content: &str,
        embedding: &[f32],
    ) -> Result<i64, DbError> {
        if embedding.len() != self.model.dimensions {
            return Err(DbError::DimensionMismatch {
                expected: self.model.dimensions,
                actual: embedding.len(),
            });
        }
        let (start, end) = range.start_end_i64()?;
        let blob: Vec<u8> = bytemuck::cast_slice(embedding).to_vec();
        let sql = format!(
            "INSERT INTO {table} (document_id, range_kind, range_start, range_end, content, embedding)
             VALUES (?, ?, ?, ?, ?, ?)",
            table = self.chunk_table,
        );
        self.conn
            .execute(
                &sql,
                turso::params::Params::Positional(vec![
                    Value::Integer(document_id),
                    Value::Text(range.kind().to_string()),
                    Value::Integer(start),
                    Value::Integer(end),
                    Value::Text(content.to_string()),
                    Value::Blob(blob),
                ]),
            )
            .await
            .map_err(|source| DbError::Query {
                op: "create_chunk",
                source,
            })?;
        Ok(self.conn.last_insert_rowid())
    }

    /// K-nearest chunks under cosine distance. `tags`, when non-empty, applies
    /// ALL-of semantics — only chunks whose document carries every listed tag
    /// are considered. An empty slice means "no tag filter".
    pub async fn search_chunks(
        &self,
        query: &[f32],
        limit: usize,
        tags: &[&str],
    ) -> Result<Vec<ChunkMatch>, DbError> {
        if query.len() != self.model.dimensions {
            return Err(DbError::DimensionMismatch {
                expected: self.model.dimensions,
                actual: query.len(),
            });
        }
        let blob: Vec<u8> = bytemuck::cast_slice(query).to_vec();

        // turso supports vector_distance_cos(blob, blob) natively, so ordering
        // happens in the DB. No KNN index is available in 0.4, so this scans
        // the chunk table — fine for small collections. ALL-of tag semantics
        // mirror `list_conversations`: JOIN, GROUP BY, HAVING COUNT(DISTINCT).
        let (sql, params) = if tags.is_empty() {
            let sql = format!(
                "SELECT c.id, c.document_id, c.content, c.range_kind, c.range_start, c.range_end,
                        vector_distance_cos(c.embedding, ?) AS dist
                 FROM {table} c
                 ORDER BY dist ASC
                 LIMIT ?",
                table = self.chunk_table,
            );
            let params = turso::params::Params::Positional(vec![
                Value::Blob(blob),
                Value::Integer(limit as i64),
            ]);
            (sql, params)
        } else {
            let placeholders = vec!["?"; tags.len()].join(", ");
            let sql = format!(
                "SELECT c.id, c.document_id, c.content, c.range_kind, c.range_start, c.range_end,
                        vector_distance_cos(c.embedding, ?) AS dist
                 FROM {table} c
                 INNER JOIN document_tags dt ON dt.document_id = c.document_id
                 INNER JOIN tags t           ON t.id = dt.tag_id
                 WHERE t.name IN ({placeholders})
                 GROUP BY c.id, c.document_id, c.content, c.range_kind, c.range_start, c.range_end, dist
                 HAVING COUNT(DISTINCT t.id) = ?
                 ORDER BY dist ASC
                 LIMIT ?",
                table = self.chunk_table,
            );
            let mut positional: Vec<Value> = Vec::with_capacity(tags.len() + 3);
            positional.push(Value::Blob(blob));
            for tag in tags {
                positional.push(Value::Text((*tag).to_string()));
            }
            positional.push(Value::Integer(tags.len() as i64));
            positional.push(Value::Integer(limit as i64));
            (sql, turso::params::Params::Positional(positional))
        };

        let mut rows = self
            .conn
            .query(&sql, params)
            .await
            .map_err(|source| DbError::Query {
                op: "search_chunks",
                source,
            })?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await.map_err(|source| DbError::Query {
            op: "search_chunks.next",
            source,
        })? {
            let chunk_id: i64 = row.get(0).map_err(|source| DbError::Query {
                op: "search_chunks.get.id",
                source,
            })?;
            let document_id: i64 = row.get(1).map_err(|source| DbError::Query {
                op: "search_chunks.get.doc",
                source,
            })?;
            let content: String = row.get(2).map_err(|source| DbError::Query {
                op: "search_chunks.get.content",
                source,
            })?;
            let range_kind: String = row.get(3).map_err(|source| DbError::Query {
                op: "search_chunks.get.kind",
                source,
            })?;
            let range_start: i64 = row.get(4).map_err(|source| DbError::Query {
                op: "search_chunks.get.start",
                source,
            })?;
            let range_end: i64 = row.get(5).map_err(|source| DbError::Query {
                op: "search_chunks.get.end",
                source,
            })?;
            let distance: f64 = row.get(6).map_err(|source| DbError::Query {
                op: "search_chunks.get.dist",
                source,
            })?;
            out.push(ChunkMatch {
                chunk_id,
                document_id,
                content,
                range: ChunkRange::from_db(&range_kind, range_start, range_end)?,
                distance: distance as f32,
            });
        }
        Ok(out)
    }

    // ---- Conversations ----------------------------------------------------

    pub async fn create_conversation(&self, id: &str, title: Option<&str>) -> Result<(), DbError> {
        self.conn
            .execute(
                "INSERT INTO conversations (id, title) VALUES (?, ?)",
                (
                    id.to_string(),
                    title
                        .map(|t| Value::Text(t.to_string()))
                        .unwrap_or(Value::Null),
                ),
            )
            .await
            .map_err(|source| DbError::Query {
                op: "create_conversation",
                source,
            })?;
        Ok(())
    }

    pub async fn get_conversation(&self, id: &str) -> Result<Option<Conversation>, DbError> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, title, created_at, updated_at FROM conversations WHERE id = ?",
                (id.to_string(),),
            )
            .await
            .map_err(|source| DbError::Query {
                op: "get_conversation",
                source,
            })?;
        match rows.next().await.map_err(|source| DbError::Query {
            op: "get_conversation.next",
            source,
        })? {
            Some(row) => Ok(Some(row_to_conversation(&row)?)),
            None => Ok(None),
        }
    }

    pub async fn list_conversations(
        &self,
        filter: &ConversationFilter<'_>,
    ) -> Result<Vec<Conversation>, DbError> {
        // Build SQL progressively. Turso 0.4 doesn't support `IN (subquery)`,
        // but `GROUP BY ... HAVING COUNT(DISTINCT ...) = N` works and gives
        // us ALL-of tag semantics in a single statement.
        let mut sql =
            String::from("SELECT c.id, c.title, c.created_at, c.updated_at FROM conversations c");
        let mut params: Vec<Value> = Vec::new();

        if !filter.tags.is_empty() {
            sql.push_str(
                " INNER JOIN conversation_tags ct ON ct.conversation_id = c.id\
                 \n INNER JOIN tags t ON t.id = ct.tag_id",
            );
        }

        if filter.text_query.is_some() {
            sql.push_str(" INNER JOIN conversation_messages cm ON cm.conversation_id = c.id");
        }

        let mut where_clauses: Vec<String> = Vec::new();

        if !filter.tags.is_empty() {
            let placeholders = vec!["?"; filter.tags.len()].join(", ");
            where_clauses.push(format!("t.name IN ({placeholders})"));
            for tag in filter.tags {
                params.push(Value::Text((*tag).to_string()));
            }
        }

        if let Some(q) = filter.text_query {
            let escaped = q
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_");
            let pattern = format!("%{escaped}%");
            where_clauses.push("cm.content LIKE ? ESCAPE '\\'".to_string());
            params.push(Value::Text(pattern));
        }

        if !where_clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&where_clauses.join(" AND "));
        }

        sql.push_str(" GROUP BY c.id, c.title, c.created_at, c.updated_at");

        if !filter.tags.is_empty() {
            sql.push_str(" HAVING COUNT(DISTINCT t.id) = ?");
            params.push(Value::Integer(filter.tags.len() as i64));
        }

        sql.push_str(" ORDER BY c.updated_at DESC");

        if let Some(limit) = filter.limit {
            sql.push_str(" LIMIT ?");
            params.push(Value::Integer(limit as i64));
        }

        let mut rows = self
            .conn
            .query(&sql, turso::params::Params::Positional(params))
            .await
            .map_err(|source| DbError::Query {
                op: "list_conversations",
                source,
            })?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await.map_err(|source| DbError::Query {
            op: "list_conversations.next",
            source,
        })? {
            out.push(row_to_conversation(&row)?);
        }
        Ok(out)
    }

    pub async fn delete_conversation(&self, id: &str) -> Result<(), DbError> {
        let affected = self
            .conn
            .execute("DELETE FROM conversations WHERE id = ?", (id.to_string(),))
            .await
            .map_err(|source| DbError::Query {
                op: "delete_conversation",
                source,
            })?;
        if affected == 0 {
            return Err(DbError::NotFound {
                kind: "conversation",
                id: 0,
            });
        }
        Ok(())
    }

    pub async fn touch_conversation(&self, id: &str) -> Result<(), DbError> {
        self.conn
            .execute(
                "UPDATE conversations SET updated_at = datetime('now') WHERE id = ?",
                (id.to_string(),),
            )
            .await
            .map_err(|source| DbError::Query {
                op: "touch_conversation",
                source,
            })?;
        Ok(())
    }

    pub async fn set_conversation_title(
        &self,
        id: &str,
        title: Option<&str>,
    ) -> Result<(), DbError> {
        let affected = self
            .conn
            .execute(
                "UPDATE conversations SET title = ?, updated_at = datetime('now') WHERE id = ?",
                (
                    title
                        .map(|t| Value::Text(t.to_string()))
                        .unwrap_or(Value::Null),
                    Value::Text(id.to_string()),
                ),
            )
            .await
            .map_err(|source| DbError::Query {
                op: "set_conversation_title",
                source,
            })?;
        if affected == 0 {
            return Err(DbError::NotFound {
                kind: "conversation",
                id: 0,
            });
        }
        Ok(())
    }

    pub async fn add_conversation_tag(&self, conv_id: &str, tag: &str) -> Result<(), DbError> {
        let tag_id = self.upsert_tag(tag).await?;
        self.conn
            .execute(
                "INSERT OR IGNORE INTO conversation_tags (conversation_id, tag_id) VALUES (?, ?)",
                (conv_id.to_string(), tag_id),
            )
            .await
            .map_err(|source| DbError::Query {
                op: "add_conversation_tag.link",
                source,
            })?;
        Ok(())
    }

    pub async fn remove_conversation_tag(&self, conv_id: &str, tag: &str) -> Result<(), DbError> {
        // Two-query: turso 0.4 has no `IN (subquery)`. Missing tag is a no-op.
        let mut rows = self
            .conn
            .query("SELECT id FROM tags WHERE name = ?", (tag.to_string(),))
            .await
            .map_err(|source| DbError::Query {
                op: "remove_conversation_tag.select_tag",
                source,
            })?;
        let tag_id: Option<i64> = match rows.next().await.map_err(|source| DbError::Query {
            op: "remove_conversation_tag.select_tag.next",
            source,
        })? {
            Some(row) => Some(row.get(0).map_err(|source| DbError::Query {
                op: "remove_conversation_tag.select_tag.get",
                source,
            })?),
            None => None,
        };
        let Some(tag_id) = tag_id else {
            return Ok(());
        };
        self.conn
            .execute(
                "DELETE FROM conversation_tags WHERE conversation_id = ? AND tag_id = ?",
                (conv_id.to_string(), tag_id),
            )
            .await
            .map_err(|source| DbError::Query {
                op: "remove_conversation_tag.delete",
                source,
            })?;
        Ok(())
    }

    pub async fn tags_for_conversation(&self, conv_id: &str) -> Result<Vec<String>, DbError> {
        let mut rows = self
            .conn
            .query(
                "SELECT t.name
                 FROM tags t
                 INNER JOIN conversation_tags ct ON ct.tag_id = t.id
                 WHERE ct.conversation_id = ?
                 ORDER BY t.name",
                (conv_id.to_string(),),
            )
            .await
            .map_err(|source| DbError::Query {
                op: "tags_for_conversation",
                source,
            })?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await.map_err(|source| DbError::Query {
            op: "tags_for_conversation.next",
            source,
        })? {
            let name: String = row.get(0).map_err(|source| DbError::Query {
                op: "tags_for_conversation.get",
                source,
            })?;
            out.push(name);
        }
        Ok(out)
    }

    /// All tag names across the whole database, alphabetically. Used by the
    /// TUI tag editor's autocomplete.
    pub async fn list_all_tags(&self) -> Result<Vec<String>, DbError> {
        let mut rows = self
            .conn
            .query("SELECT name FROM tags ORDER BY name", ())
            .await
            .map_err(|source| DbError::Query {
                op: "list_all_tags",
                source,
            })?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await.map_err(|source| DbError::Query {
            op: "list_all_tags.next",
            source,
        })? {
            let name: String = row.get(0).map_err(|source| DbError::Query {
                op: "list_all_tags.get",
                source,
            })?;
            out.push(name);
        }
        Ok(out)
    }

    /// Append a message to a conversation and bump `updated_at` so the list
    /// screen sees the conversation move to the top. Role / metadata
    /// invariants are enforced here so bad pairings never land on disk.
    pub async fn append_message(
        &self,
        conv_id: &str,
        role: MessageRole,
        content: &str,
        metadata: Option<&MessageMetadata>,
    ) -> Result<i64, DbError> {
        match (role, metadata) {
            (MessageRole::ToolUse, Some(m)) | (MessageRole::Tool, Some(m))
                if !m.matches_role(role) =>
            {
                return Err(DbError::InvalidMessage {
                    reason: format!("metadata variant does not match role {:?}", role.as_str()),
                });
            }
            (MessageRole::ToolUse, None) => {
                return Err(DbError::InvalidMessage {
                    reason: "tool_use rows require ToolUse metadata".into(),
                });
            }
            (MessageRole::Tool, None) => {
                return Err(DbError::InvalidMessage {
                    reason: "tool rows require Tool metadata".into(),
                });
            }
            (MessageRole::System | MessageRole::User | MessageRole::Assistant, Some(_)) => {
                return Err(DbError::InvalidMessage {
                    reason: format!("role {} does not accept metadata", role.as_str()),
                });
            }
            _ => {}
        }

        let metadata_json = match metadata {
            Some(m) => {
                Value::Text(
                    serde_json::to_string(m).map_err(|e| DbError::InvalidMessage {
                        reason: format!("metadata serialization failed: {e}"),
                    })?,
                )
            }
            None => Value::Null,
        };

        self.conn
            .execute(
                "INSERT INTO conversation_messages (conversation_id, role, content, metadata)
                 VALUES (?, ?, ?, ?)",
                turso::params::Params::Positional(vec![
                    Value::Text(conv_id.to_string()),
                    Value::Text(role.as_str().to_string()),
                    Value::Text(content.to_string()),
                    metadata_json,
                ]),
            )
            .await
            .map_err(|source| DbError::Query {
                op: "append_message",
                source,
            })?;
        let id = self.conn.last_insert_rowid();

        self.touch_conversation(conv_id).await?;
        Ok(id)
    }

    pub async fn messages_for_conversation(
        &self,
        conv_id: &str,
    ) -> Result<Vec<StoredMessage>, DbError> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, conversation_id, role, content, metadata, created_at
                 FROM conversation_messages
                 WHERE conversation_id = ?
                 ORDER BY id",
                (conv_id.to_string(),),
            )
            .await
            .map_err(|source| DbError::Query {
                op: "messages_for_conversation",
                source,
            })?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await.map_err(|source| DbError::Query {
            op: "messages_for_conversation.next",
            source,
        })? {
            out.push(row_to_message(&row)?);
        }
        Ok(out)
    }
}

fn row_to_conversation(row: &turso::Row) -> Result<Conversation, DbError> {
    let title: Option<String> = match row.get_value(1).map_err(|source| DbError::Query {
        op: "row_to_conversation.title",
        source,
    })? {
        Value::Text(t) => Some(t),
        Value::Null => None,
        other => {
            return Err(DbError::InvalidMessage {
                reason: format!("unexpected type for conversations.title: {other:?}"),
            });
        }
    };
    Ok(Conversation {
        id: row.get(0).map_err(|source| DbError::Query {
            op: "row_to_conversation.id",
            source,
        })?,
        title,
        created_at: row.get(2).map_err(|source| DbError::Query {
            op: "row_to_conversation.created_at",
            source,
        })?,
        updated_at: row.get(3).map_err(|source| DbError::Query {
            op: "row_to_conversation.updated_at",
            source,
        })?,
    })
}

fn row_to_message(row: &turso::Row) -> Result<StoredMessage, DbError> {
    let role_str: String = row.get(2).map_err(|source| DbError::Query {
        op: "row_to_message.role",
        source,
    })?;
    let role = MessageRole::from_db(&role_str)?;
    let metadata_json: Option<String> = match row.get_value(4).map_err(|source| DbError::Query {
        op: "row_to_message.metadata",
        source,
    })? {
        Value::Text(t) => Some(t),
        Value::Null => None,
        other => {
            return Err(DbError::InvalidMessage {
                reason: format!("unexpected type for metadata: {other:?}"),
            });
        }
    };
    let metadata = match metadata_json {
        Some(s) => Some(serde_json::from_str::<MessageMetadata>(&s).map_err(|e| {
            DbError::InvalidMessage {
                reason: format!("metadata deserialization failed: {e}"),
            }
        })?),
        None => None,
    };
    if let Some(ref m) = metadata
        && !m.matches_role(role)
    {
        return Err(DbError::InvalidMessage {
            reason: format!("stored metadata variant does not match role {role_str:?}"),
        });
    }
    Ok(StoredMessage {
        id: row.get(0).map_err(|source| DbError::Query {
            op: "row_to_message.id",
            source,
        })?,
        conversation_id: row.get(1).map_err(|source| DbError::Query {
            op: "row_to_message.conversation_id",
            source,
        })?,
        role,
        content: row.get(3).map_err(|source| DbError::Query {
            op: "row_to_message.content",
            source,
        })?,
        metadata,
        created_at: row.get(5).map_err(|source| DbError::Query {
            op: "row_to_message.created_at",
            source,
        })?,
    })
}

fn row_to_document(row: &turso::Row) -> Result<Document, DbError> {
    Ok(Document {
        id: row.get(0).map_err(|source| DbError::Query {
            op: "row_to_document.id",
            source,
        })?,
        path: row.get(1).map_err(|source| DbError::Query {
            op: "row_to_document.path",
            source,
        })?,
        created_at: row.get(2).map_err(|source| DbError::Query {
            op: "row_to_document.created_at",
            source,
        })?,
        updated_at: row.get(3).map_err(|source| DbError::Query {
            op: "row_to_document.updated_at",
            source,
        })?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn model_name(dims: usize) -> String {
        format!("test-{dims}")
    }

    /// Test helper: open a DAL at `path` with a probe that always returns
    /// `dims`. The cache table will be populated on the first open and
    /// short-circuit on subsequent ones.
    async fn open_with_dims(path: &std::path::Path, dims: usize) -> Dal {
        Dal::open(path, model_name(dims), || async move {
            Ok(schema::EmbeddingProbe {
                dimensions: dims,
                max_input_tokens: None,
            })
        })
        .await
        .unwrap()
    }

    async fn mem_dal(dims: usize) -> Dal {
        // turso doesn't expose a way to share an in-memory DB between calls,
        // so each test uses its own tempfile. Cheap enough.
        let dir = TempDir::new().unwrap();
        let path: PathBuf = dir.path().join("t.db");
        // Leak the tempdir so files survive the async calls below. Each test
        // gets a fresh one.
        std::mem::forget(dir);
        open_with_dims(&path, dims).await
    }

    #[tokio::test]
    async fn probe_runs_on_cache_miss_then_is_cached() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let dir = TempDir::new().unwrap();
        let path: PathBuf = dir.path().join("t.db");

        let calls = Arc::new(AtomicUsize::new(0));

        let calls_a = calls.clone();
        let dal_a = Dal::open(&path, "probe-test".into(), move || async move {
            calls_a.fetch_add(1, Ordering::SeqCst);
            Ok(schema::EmbeddingProbe {
                dimensions: 7,
                max_input_tokens: Some(512),
            })
        })
        .await
        .unwrap();
        assert_eq!(dal_a.model().dimensions, 7);
        assert_eq!(dal_a.model().max_input_tokens, Some(512));
        assert_eq!(calls.load(Ordering::SeqCst), 1, "probe runs on first open");
        drop(dal_a);

        let calls_b = calls.clone();
        let dal_b = Dal::open(&path, "probe-test".into(), move || async move {
            calls_b.fetch_add(1, Ordering::SeqCst);
            panic!("probe must not run on cache hit");
        })
        .await
        .unwrap();
        assert_eq!(dal_b.model().dimensions, 7);
        assert_eq!(dal_b.model().max_input_tokens, Some(512));
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "probe must not run on cache hit",
        );
    }

    #[tokio::test]
    async fn document_crud_round_trip() {
        let dal = mem_dal(4).await;
        let id = dal.create_document("/docs/a.pdf").await.unwrap();
        let got = dal.get_document(id).await.unwrap().unwrap();
        assert_eq!(got.path, "/docs/a.pdf");

        dal.update_document_path(id, "/docs/renamed.pdf")
            .await
            .unwrap();
        let got = dal.get_document(id).await.unwrap().unwrap();
        assert_eq!(got.path, "/docs/renamed.pdf");

        let by_path = dal
            .get_document_by_path("/docs/renamed.pdf")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(by_path.id, id);

        assert_eq!(dal.list_documents().await.unwrap().len(), 1);

        dal.delete_document(id).await.unwrap();
        assert!(dal.get_document(id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn list_documents_filtered_all_of_tags() {
        let dal = mem_dal(4).await;
        let a = dal.create_document("/a").await.unwrap();
        let b = dal.create_document("/b").await.unwrap();
        let c = dal.create_document("/c").await.unwrap();
        dal.set_tags(a, &["x", "y"]).await.unwrap();
        dal.set_tags(b, &["x"]).await.unwrap();
        dal.set_tags(c, &["y"]).await.unwrap();

        let hits = dal
            .list_documents_filtered(&DocumentFilter {
                tags: &["x", "y"],
                limit: None,
            })
            .await
            .unwrap();
        let ids: Vec<i64> = hits.iter().map(|d| d.id).collect();
        assert_eq!(ids, vec![a]);

        // Single tag acts as OR=1 (only those matching that tag).
        let x_only = dal
            .list_documents_filtered(&DocumentFilter {
                tags: &["x"],
                limit: None,
            })
            .await
            .unwrap();
        let mut x_ids: Vec<i64> = x_only.iter().map(|d| d.id).collect();
        x_ids.sort();
        assert_eq!(x_ids, vec![a, b]);

        // No tag filter returns all.
        assert_eq!(dal.list_documents().await.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn delete_missing_document_is_not_found() {
        let dal = mem_dal(4).await;
        match dal.delete_document(999).await {
            Err(DbError::NotFound {
                kind: "document",
                id: 999,
            }) => {}
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn duplicate_document_path_errors() {
        let dal = mem_dal(4).await;
        dal.create_document("/x").await.unwrap();
        let err = dal.create_document("/x").await.unwrap_err();
        assert!(matches!(
            err,
            DbError::Query {
                op: "create_document",
                ..
            }
        ));
    }

    #[tokio::test]
    async fn set_tags_replaces_and_lists() {
        let dal = mem_dal(4).await;
        let id = dal.create_document("/x").await.unwrap();

        dal.set_tags(id, &["a", "b"]).await.unwrap();
        let mut t1 = dal.tags_for_document(id).await.unwrap();
        t1.sort();
        assert_eq!(t1, vec!["a", "b"]);

        dal.set_tags(id, &["b", "c"]).await.unwrap();
        let mut t2 = dal.tags_for_document(id).await.unwrap();
        t2.sort();
        assert_eq!(t2, vec!["b", "c"]);
    }

    #[tokio::test]
    async fn documents_with_tag_returns_shared_docs() {
        let dal = mem_dal(4).await;
        let a = dal.create_document("/a").await.unwrap();
        let b = dal.create_document("/b").await.unwrap();
        let c = dal.create_document("/c").await.unwrap();
        dal.set_tags(a, &["shared"]).await.unwrap();
        dal.set_tags(b, &["shared", "other"]).await.unwrap();
        dal.set_tags(c, &["other"]).await.unwrap();

        let mut shared: Vec<i64> = dal
            .documents_with_tag("shared")
            .await
            .unwrap()
            .into_iter()
            .map(|d| d.id)
            .collect();
        shared.sort();
        assert_eq!(shared, vec![a, b]);
    }

    #[tokio::test]
    async fn add_and_remove_tag() {
        let dal = mem_dal(4).await;
        let id = dal.create_document("/x").await.unwrap();
        dal.add_tag(id, "alpha").await.unwrap();
        dal.add_tag(id, "alpha").await.unwrap(); // no-op, no error
        dal.add_tag(id, "beta").await.unwrap();

        let mut tags = dal.tags_for_document(id).await.unwrap();
        tags.sort();
        assert_eq!(tags, vec!["alpha", "beta"]);

        dal.remove_tag(id, "alpha").await.unwrap();
        assert_eq!(dal.tags_for_document(id).await.unwrap(), vec!["beta"]);
    }

    #[tokio::test]
    async fn search_tags_prefix_matches() {
        let dal = mem_dal(4).await;
        let id = dal.create_document("/x").await.unwrap();
        dal.set_tags(id, &["fantasy", "fiction", "history"])
            .await
            .unwrap();

        let mut hits = dal.search_tags("fi", 10).await.unwrap();
        hits.sort();
        assert_eq!(hits, vec!["fiction"]);

        let all_f = dal.search_tags("f", 10).await.unwrap();
        assert_eq!(all_f, vec!["fantasy", "fiction"]);
    }

    #[tokio::test]
    async fn create_chunk_rejects_wrong_dimension() {
        let dal = mem_dal(4).await;
        let doc = dal.create_document("/x").await.unwrap();
        let err = dal
            .create_chunk(
                doc,
                ChunkRange::Pages { start: 1, end: 2 },
                "hello",
                &[1.0, 2.0, 3.0], // 3, not 4
            )
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            DbError::DimensionMismatch {
                expected: 4,
                actual: 3,
            }
        ));
    }

    #[tokio::test]
    async fn search_chunks_orders_by_distance() {
        let dal = mem_dal(4).await;
        let doc = dal.create_document("/x").await.unwrap();

        // Three chunks: one close to the query, one medium, one far.
        let close = dal
            .create_chunk(
                doc,
                ChunkRange::Pages { start: 1, end: 1 },
                "near",
                &[1.0, 0.0, 0.0, 0.0],
            )
            .await
            .unwrap();
        let _med = dal
            .create_chunk(
                doc,
                ChunkRange::Pages { start: 2, end: 2 },
                "mid",
                &[0.7, 0.7, 0.0, 0.0],
            )
            .await
            .unwrap();
        let _far = dal
            .create_chunk(
                doc,
                ChunkRange::Pages { start: 3, end: 3 },
                "far",
                &[0.0, 1.0, 0.0, 0.0],
            )
            .await
            .unwrap();

        let hits = dal
            .search_chunks(&[1.0, 0.0, 0.0, 0.0], 3, &[])
            .await
            .unwrap();
        assert_eq!(hits.len(), 3);
        assert_eq!(hits[0].chunk_id, close);
        // Cosine distance to self is ~0.
        assert!(hits[0].distance < 1e-4);
        // Distances are monotonically non-decreasing.
        assert!(hits[0].distance <= hits[1].distance);
        assert!(hits[1].distance <= hits[2].distance);
    }

    #[tokio::test]
    async fn search_chunks_tag_filter_scopes_results() {
        let dal = mem_dal(4).await;
        let a = dal.create_document("/a").await.unwrap();
        let b = dal.create_document("/b").await.unwrap();
        dal.set_tags(a, &["keep"]).await.unwrap();
        dal.set_tags(b, &["drop"]).await.unwrap();

        let c_a = dal
            .create_chunk(
                a,
                ChunkRange::Pages { start: 1, end: 1 },
                "from-a",
                &[1.0, 0.0, 0.0, 0.0],
            )
            .await
            .unwrap();
        let _c_b = dal
            .create_chunk(
                b,
                ChunkRange::Pages { start: 1, end: 1 },
                "from-b",
                &[1.0, 0.0, 0.0, 0.0],
            )
            .await
            .unwrap();

        let hits = dal
            .search_chunks(&[1.0, 0.0, 0.0, 0.0], 10, &["keep"])
            .await
            .unwrap();
        let ids: Vec<i64> = hits.iter().map(|h| h.chunk_id).collect();
        assert_eq!(ids, vec![c_a]);
    }

    #[tokio::test]
    async fn search_chunks_all_of_tag_filter() {
        let dal = mem_dal(4).await;
        let a = dal.create_document("/a").await.unwrap();
        let b = dal.create_document("/b").await.unwrap();
        let c = dal.create_document("/c").await.unwrap();
        // Only `a` has both tags. `b` has only `x`, `c` has only `y`.
        dal.set_tags(a, &["x", "y"]).await.unwrap();
        dal.set_tags(b, &["x"]).await.unwrap();
        dal.set_tags(c, &["y"]).await.unwrap();

        for doc in [a, b, c] {
            dal.create_chunk(
                doc,
                ChunkRange::Pages { start: 1, end: 1 },
                "c",
                &[1.0, 0.0, 0.0, 0.0],
            )
            .await
            .unwrap();
        }

        let hits = dal
            .search_chunks(&[1.0, 0.0, 0.0, 0.0], 10, &["x", "y"])
            .await
            .unwrap();
        let docs: Vec<i64> = hits.iter().map(|h| h.document_id).collect();
        assert_eq!(docs, vec![a]);
    }

    #[tokio::test]
    async fn byte_range_round_trips() {
        let dal = mem_dal(4).await;
        let doc = dal.create_document("/x").await.unwrap();
        dal.create_chunk(
            doc,
            ChunkRange::Bytes {
                start: 100,
                end: 200,
            },
            "hello",
            &[0.0, 0.0, 0.0, 1.0],
        )
        .await
        .unwrap();

        let hits = dal
            .search_chunks(&[0.0, 0.0, 0.0, 1.0], 1, &[])
            .await
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(
            hits[0].range,
            ChunkRange::Bytes {
                start: 100,
                end: 200,
            }
        );
    }

    #[tokio::test]
    async fn different_dimensions_use_disjoint_tables() {
        // Open two DALs against the *same* DB file, one with 4 dims and one
        // with 8. They must not collide, and inserts into one must not
        // appear in the other.
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("t.db");

        let dal_4 = open_with_dims(&path, 4).await;
        let doc = dal_4.create_document("/x").await.unwrap();
        let chunk_4 = dal_4
            .create_chunk(
                doc,
                ChunkRange::Pages { start: 1, end: 1 },
                "four",
                &[1.0, 0.0, 0.0, 0.0],
            )
            .await
            .unwrap();
        drop(dal_4);

        let dal_8 = open_with_dims(&path, 8).await;
        // The 8-dim chunk table exists but is empty.
        let hits_8 = dal_8
            .search_chunks(&[1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], 10, &[])
            .await
            .unwrap();
        assert!(hits_8.is_empty(), "8-dim table should not see 4-dim chunks");

        // Re-open the 4-dim DAL and confirm the chunk still lives there.
        drop(dal_8);
        let dal_4_again = open_with_dims(&path, 4).await;
        let hits_4 = dal_4_again
            .search_chunks(&[1.0, 0.0, 0.0, 0.0], 10, &[])
            .await
            .unwrap();
        assert_eq!(hits_4.len(), 1);
        assert_eq!(hits_4[0].chunk_id, chunk_4);
    }

    // ---- Conversations -----------------------------------------------------

    fn uuid() -> String {
        uuid::Uuid::now_v7().hyphenated().to_string()
    }

    #[tokio::test]
    async fn conversation_create_get_delete() {
        let dal = mem_dal(4).await;
        let id = uuid();
        dal.create_conversation(&id, Some("first")).await.unwrap();
        let got = dal.get_conversation(&id).await.unwrap().unwrap();
        assert_eq!(got.id, id);
        assert_eq!(got.title.as_deref(), Some("first"));

        dal.delete_conversation(&id).await.unwrap();
        assert!(dal.get_conversation(&id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn conversation_list_orders_by_updated_at_desc() {
        let dal = mem_dal(4).await;
        let a = uuid();
        let b = uuid();
        dal.create_conversation(&a, None).await.unwrap();
        // Force a > b ordering via an explicit touch after creating b.
        dal.create_conversation(&b, None).await.unwrap();
        // Sleep-free ordering: append a message to `a` last so its
        // updated_at is strictly later.
        dal.append_message(&a, MessageRole::User, "hello", None)
            .await
            .unwrap();

        let list = dal
            .list_conversations(&ConversationFilter::default())
            .await
            .unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, a);
        assert_eq!(list[1].id, b);
    }

    #[tokio::test]
    async fn conversation_tag_and_filter_all_of() {
        let dal = mem_dal(4).await;
        let a = uuid();
        let b = uuid();
        let c = uuid();
        dal.create_conversation(&a, None).await.unwrap();
        dal.create_conversation(&b, None).await.unwrap();
        dal.create_conversation(&c, None).await.unwrap();

        dal.add_conversation_tag(&a, "work").await.unwrap();
        dal.add_conversation_tag(&a, "urgent").await.unwrap();
        dal.add_conversation_tag(&b, "work").await.unwrap();
        dal.add_conversation_tag(&c, "urgent").await.unwrap();

        // Only `a` has both.
        let both = dal
            .list_conversations(&ConversationFilter {
                tags: &["work", "urgent"],
                ..Default::default()
            })
            .await
            .unwrap();
        let ids: Vec<String> = both.into_iter().map(|c| c.id).collect();
        assert_eq!(ids, vec![a.clone()]);

        // `work` alone: a and b.
        let mut work_ids: Vec<String> = dal
            .list_conversations(&ConversationFilter {
                tags: &["work"],
                ..Default::default()
            })
            .await
            .unwrap()
            .into_iter()
            .map(|c| c.id)
            .collect();
        work_ids.sort();
        let mut expected = vec![a.clone(), b.clone()];
        expected.sort();
        assert_eq!(work_ids, expected);
    }

    #[tokio::test]
    async fn conversation_tag_add_idempotent_and_remove_missing_is_noop() {
        let dal = mem_dal(4).await;
        let id = uuid();
        dal.create_conversation(&id, None).await.unwrap();

        dal.add_conversation_tag(&id, "alpha").await.unwrap();
        dal.add_conversation_tag(&id, "alpha").await.unwrap();
        let tags = dal.tags_for_conversation(&id).await.unwrap();
        assert_eq!(tags, vec!["alpha"]);

        // Removing a tag that was never created must not error.
        dal.remove_conversation_tag(&id, "never-existed")
            .await
            .unwrap();
        dal.remove_conversation_tag(&id, "alpha").await.unwrap();
        assert!(dal.tags_for_conversation(&id).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn conversation_text_search_matches_message_content() {
        let dal = mem_dal(4).await;
        let a = uuid();
        let b = uuid();
        dal.create_conversation(&a, None).await.unwrap();
        dal.create_conversation(&b, None).await.unwrap();

        dal.append_message(&a, MessageRole::User, "what is rust?", None)
            .await
            .unwrap();
        dal.append_message(&b, MessageRole::User, "what is python?", None)
            .await
            .unwrap();

        let hits = dal
            .list_conversations(&ConversationFilter {
                text_query: Some("rust"),
                ..Default::default()
            })
            .await
            .unwrap();
        let ids: Vec<String> = hits.into_iter().map(|c| c.id).collect();
        assert_eq!(ids, vec![a]);
    }

    #[tokio::test]
    async fn delete_conversation_cascades_to_messages_and_tags() {
        let dal = mem_dal(4).await;
        let id = uuid();
        dal.create_conversation(&id, None).await.unwrap();
        dal.add_conversation_tag(&id, "t1").await.unwrap();
        dal.append_message(&id, MessageRole::User, "hi", None)
            .await
            .unwrap();

        dal.delete_conversation(&id).await.unwrap();

        // Messages and tag links are gone (cascade).
        assert!(dal.messages_for_conversation(&id).await.unwrap().is_empty());
        assert!(dal.tags_for_conversation(&id).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn delete_missing_conversation_is_not_found() {
        let dal = mem_dal(4).await;
        let err = dal.delete_conversation("does-not-exist").await.unwrap_err();
        assert!(matches!(
            err,
            DbError::NotFound {
                kind: "conversation",
                ..
            }
        ));
    }

    #[tokio::test]
    async fn append_message_round_trips_all_roles() {
        let dal = mem_dal(4).await;
        let id = uuid();
        dal.create_conversation(&id, None).await.unwrap();

        dal.append_message(&id, MessageRole::System, "sys prompt", None)
            .await
            .unwrap();
        dal.append_message(&id, MessageRole::User, "hi", None)
            .await
            .unwrap();
        dal.append_message(&id, MessageRole::Assistant, "hello", None)
            .await
            .unwrap();
        dal.append_message(
            &id,
            MessageRole::ToolUse,
            "",
            Some(&MessageMetadata::ToolUse {
                tool_call_id: "call-1".into(),
                name: "search".into(),
                arguments: serde_json::json!({"q": "rust"}),
            }),
        )
        .await
        .unwrap();
        dal.append_message(
            &id,
            MessageRole::Tool,
            "result text",
            Some(&MessageMetadata::Tool {
                tool_call_id: "call-1".into(),
            }),
        )
        .await
        .unwrap();

        let msgs = dal.messages_for_conversation(&id).await.unwrap();
        assert_eq!(msgs.len(), 5);
        assert_eq!(msgs[0].role, MessageRole::System);
        assert_eq!(msgs[1].role, MessageRole::User);
        assert_eq!(msgs[2].role, MessageRole::Assistant);
        assert_eq!(msgs[3].role, MessageRole::ToolUse);
        assert!(matches!(
            msgs[3].metadata,
            Some(MessageMetadata::ToolUse { .. })
        ));
        assert_eq!(msgs[4].role, MessageRole::Tool);
        assert!(matches!(
            msgs[4].metadata,
            Some(MessageMetadata::Tool { .. })
        ));
    }

    #[tokio::test]
    async fn append_message_rejects_role_metadata_mismatch() {
        let dal = mem_dal(4).await;
        let id = uuid();
        dal.create_conversation(&id, None).await.unwrap();

        // tool_use role without metadata.
        let err = dal
            .append_message(&id, MessageRole::ToolUse, "", None)
            .await
            .unwrap_err();
        assert!(matches!(err, DbError::InvalidMessage { .. }));

        // user role with tool metadata.
        let err = dal
            .append_message(
                &id,
                MessageRole::User,
                "hi",
                Some(&MessageMetadata::Tool {
                    tool_call_id: "x".into(),
                }),
            )
            .await
            .unwrap_err();
        assert!(matches!(err, DbError::InvalidMessage { .. }));
    }

    #[tokio::test]
    async fn list_all_tags_returns_global_set_sorted() {
        let dal = mem_dal(4).await;
        // Mixed source: documents and conversations contribute to the same
        // tags table.
        let doc = dal.create_document("/x").await.unwrap();
        dal.add_tag(doc, "doc-only").await.unwrap();

        let conv = uuid();
        dal.create_conversation(&conv, None).await.unwrap();
        dal.add_conversation_tag(&conv, "shared").await.unwrap();
        dal.add_tag(doc, "shared").await.unwrap();

        let all = dal.list_all_tags().await.unwrap();
        assert_eq!(all, vec!["doc-only", "shared"]);
    }
}
