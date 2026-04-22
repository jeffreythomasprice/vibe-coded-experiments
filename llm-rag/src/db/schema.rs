use turso::{Connection, Value};

use super::error::DbError;

/// Metadata about the embedding model in use at startup. `dimensions` selects
/// the per-dimension chunk table (e.g. `document_chunks_768`) so swapping
/// models with different vector lengths does not collide.
/// `max_input_tokens` is the model's advertised context length in tokens,
/// sourced from the provider (e.g. Ollama's `/api/show`); `None` means the
/// provider didn't report one and callers should pick a conservative default.
#[derive(Debug, Clone)]
pub struct EmbeddingModelInfo {
    pub name: String,
    pub dimensions: usize,
    pub max_input_tokens: Option<usize>,
}

/// What a single provider probe yields: the vector length (required) plus an
/// optional max input-token count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbeddingProbe {
    pub dimensions: usize,
    pub max_input_tokens: Option<usize>,
}

/// Cached probe row for `model`. `Ok(None)` means we've never seen this model
/// and the caller should probe. If `max_input_tokens` in the cached row is
/// `NULL` (older DBs pre-dating migration 0004), callers should still probe
/// to fill it in — dimensions stays trustworthy since it's immutable per model.
pub async fn lookup_embedding_info(
    conn: &Connection,
    model: &str,
) -> Result<Option<EmbeddingProbe>, DbError> {
    let mut rows = conn
        .query(
            "SELECT dimensions, max_input_tokens FROM embedding_model_dimensions WHERE model = ?",
            (model.to_string(),),
        )
        .await
        .map_err(|source| DbError::Query {
            op: "lookup_embedding_info",
            source,
        })?;
    match rows.next().await.map_err(|source| DbError::Query {
        op: "lookup_embedding_info.next",
        source,
    })? {
        Some(row) => {
            let dims: i64 = row.get(0).map_err(|source| DbError::Query {
                op: "lookup_embedding_info.get_dims",
                source,
            })?;
            let tokens = match row.get_value(1).map_err(|source| DbError::Query {
                op: "lookup_embedding_info.get_tokens",
                source,
            })? {
                Value::Integer(v) => Some(v as usize),
                Value::Null => None,
                other => {
                    return Err(DbError::Query {
                        op: "lookup_embedding_info.get_tokens",
                        source: turso::Error::ConversionFailure(format!(
                            "unexpected type for max_input_tokens: {other:?}"
                        )),
                    });
                }
            };
            Ok(Some(EmbeddingProbe {
                dimensions: dims as usize,
                max_input_tokens: tokens,
            }))
        }
        None => Ok(None),
    }
}

/// Insert a fresh probe row. `INSERT OR IGNORE` keeps two racing processes
/// from each writing a row for the same model.
pub async fn record_embedding_info(
    conn: &Connection,
    model: &str,
    probe: EmbeddingProbe,
) -> Result<(), DbError> {
    conn.execute(
        "INSERT OR IGNORE INTO embedding_model_dimensions
            (model, dimensions, max_input_tokens)
            VALUES (?, ?, ?)",
        turso::params::Params::Positional(vec![
            Value::Text(model.to_string()),
            Value::Integer(probe.dimensions as i64),
            probe
                .max_input_tokens
                .map(|t| Value::Integer(t as i64))
                .unwrap_or(Value::Null),
        ]),
    )
    .await
    .map_err(|source| DbError::Query {
        op: "record_embedding_info",
        source,
    })?;
    Ok(())
}

/// Backfill `max_input_tokens` on an existing row whose value is NULL (older
/// DB predating migration 0004). No-op if no row matches.
pub async fn update_max_input_tokens(
    conn: &Connection,
    model: &str,
    max_input_tokens: usize,
) -> Result<(), DbError> {
    conn.execute(
        "UPDATE embedding_model_dimensions
            SET max_input_tokens = ?
            WHERE model = ? AND max_input_tokens IS NULL",
        turso::params::Params::Positional(vec![
            Value::Integer(max_input_tokens as i64),
            Value::Text(model.to_string()),
        ]),
    )
    .await
    .map_err(|source| DbError::Query {
        op: "update_max_input_tokens",
        source,
    })?;
    Ok(())
}

/// Returns the SQL table name for chunks of a given embedding dimension.
pub fn chunk_table_name(dims: usize) -> String {
    format!("document_chunks_{dims}")
}

/// Idempotently create the per-dimension chunk table and its supporting
/// index. The `embedding` column is declared `F32_BLOB(N)` so native
/// `vector_distance_cos(?, ?)` works against it; storage is still a regular
/// byte blob we serialize from `[f32]` via `bytemuck`.
///
/// Note on indexing: turso 0.4 accepts `F32_BLOB(N)` columns and the
/// `vector_distance_cos` / `vector32` functions, but rejects the
/// `libsql_vector_idx(col)` index syntax (see the probe during bring-up).
/// Searches therefore do a full scan over the chunk table. That's fine
/// for small collections; when turso grows vector-index support we can
/// layer one on without a schema migration.
pub async fn ensure_chunk_table(conn: &Connection, dims: usize) -> Result<(), DbError> {
    let table = chunk_table_name(dims);
    // range_kind values ('pages' | 'bytes') are enforced in the DAL via the
    // `ChunkRange` enum rather than a SQL CHECK — turso 0.4 doesn't parse
    // CHECK constraints yet.
    let create_table = format!(
        "CREATE TABLE IF NOT EXISTS {table} (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            document_id INTEGER NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
            range_kind  TEXT    NOT NULL,
            range_start INTEGER NOT NULL,
            range_end   INTEGER NOT NULL,
            content     TEXT    NOT NULL,
            embedding   F32_BLOB({dims}) NOT NULL,
            created_at  TEXT    NOT NULL DEFAULT (datetime('now'))
        )"
    );
    conn.execute(&create_table, ())
        .await
        .map_err(|source| DbError::Query {
            op: "ensure_chunk_table.create",
            source,
        })?;

    let create_index =
        format!("CREATE INDEX IF NOT EXISTS {table}_doc_idx ON {table} (document_id)");
    conn.execute(&create_index, ())
        .await
        .map_err(|source| DbError::Query {
            op: "ensure_chunk_table.index",
            source,
        })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_table_name_interpolates_dimensions() {
        assert_eq!(chunk_table_name(768), "document_chunks_768");
        assert_eq!(chunk_table_name(1536), "document_chunks_1536");
    }
}
