//! Server-side data access layer.
//!
//! `Dal` owns a single long-lived turso connection plus the active embedding
//! model's metadata. All writes that touch a vector are dimension-checked
//! against `self.model.dimensions` so a mismatched model can never silently
//! corrupt the per-dimension chunk table.
//!
//! Clients reach these operations over the Unix-socket RPC; the DAL itself is
//! never exposed across process boundaries.

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

pub struct Dal {
    conn: Connection,
    model: EmbeddingModelInfo,
    chunk_table: String,
}

impl Dal {
    pub async fn open(db_path: &Path, model: EmbeddingModelInfo) -> Result<Self, DbError> {
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
        schema::ensure_chunk_table(&conn, model.dimensions).await?;

        let chunk_table = schema::chunk_table_name(model.dimensions);
        Ok(Dal {
            conn,
            model,
            chunk_table,
        })
    }

    pub fn model(&self) -> &EmbeddingModelInfo {
        &self.model
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
        let mut rows = self
            .conn
            .query(
                "SELECT id, path, created_at, updated_at FROM documents ORDER BY id",
                (),
            )
            .await
            .map_err(|source| DbError::Query {
                op: "list_documents",
                source,
            })?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await.map_err(|source| DbError::Query {
            op: "list_documents.next",
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

    /// K-nearest chunks under cosine distance. `tag_filter`, when provided,
    /// restricts the scan to chunks whose document has that tag.
    pub async fn search_chunks(
        &self,
        query: &[f32],
        limit: usize,
        tag_filter: Option<&str>,
    ) -> Result<Vec<ChunkMatch>, DbError> {
        if query.len() != self.model.dimensions {
            return Err(DbError::DimensionMismatch {
                expected: self.model.dimensions,
                actual: query.len(),
            });
        }
        let blob: Vec<u8> = bytemuck::cast_slice(query).to_vec();

        // Build SQL with or without the tag-JOIN. turso supports
        // vector_distance_cos(blob, blob) natively, so ordering happens in the
        // DB. No KNN index is available in 0.4, so this scans the chunk
        // table — fine for small collections.
        let (sql, params) = match tag_filter {
            None => {
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
            }
            Some(tag) => {
                let sql = format!(
                    "SELECT c.id, c.document_id, c.content, c.range_kind, c.range_start, c.range_end,
                            vector_distance_cos(c.embedding, ?) AS dist
                     FROM {table} c
                     INNER JOIN document_tags dt ON dt.document_id = c.document_id
                     INNER JOIN tags t           ON t.id = dt.tag_id
                     WHERE t.name = ?
                     ORDER BY dist ASC
                     LIMIT ?",
                    table = self.chunk_table,
                );
                let params = turso::params::Params::Positional(vec![
                    Value::Blob(blob),
                    Value::Text(tag.to_string()),
                    Value::Integer(limit as i64),
                ]);
                (sql, params)
            }
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

    fn model(dims: usize) -> EmbeddingModelInfo {
        EmbeddingModelInfo {
            name: format!("test-{dims}"),
            dimensions: dims,
        }
    }

    async fn mem_dal(dims: usize) -> Dal {
        // turso doesn't expose a way to share an in-memory DB between calls,
        // so each test uses its own tempfile. Cheap enough.
        let dir = TempDir::new().unwrap();
        let path: PathBuf = dir.path().join("t.db");
        // Leak the tempdir so files survive the async calls below. Each test
        // gets a fresh one.
        std::mem::forget(dir);
        Dal::open(&path, model(dims)).await.unwrap()
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
            .search_chunks(&[1.0, 0.0, 0.0, 0.0], 3, None)
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
            .search_chunks(&[1.0, 0.0, 0.0, 0.0], 10, Some("keep"))
            .await
            .unwrap();
        let ids: Vec<i64> = hits.iter().map(|h| h.chunk_id).collect();
        assert_eq!(ids, vec![c_a]);
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
            .search_chunks(&[0.0, 0.0, 0.0, 1.0], 1, None)
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

        let dal_4 = Dal::open(&path, model(4)).await.unwrap();
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

        let dal_8 = Dal::open(&path, model(8)).await.unwrap();
        // The 8-dim chunk table exists but is empty.
        let hits_8 = dal_8
            .search_chunks(&[1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], 10, None)
            .await
            .unwrap();
        assert!(hits_8.is_empty(), "8-dim table should not see 4-dim chunks");

        // Re-open the 4-dim DAL and confirm the chunk still lives there.
        drop(dal_8);
        let dal_4_again = Dal::open(&path, model(4)).await.unwrap();
        let hits_4 = dal_4_again
            .search_chunks(&[1.0, 0.0, 0.0, 0.0], 10, None)
            .await
            .unwrap();
        assert_eq!(hits_4.len(), 1);
        assert_eq!(hits_4[0].chunk_id, chunk_4);
    }
}
