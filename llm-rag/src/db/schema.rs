use turso::{Connection, Value};

use super::error::DbError;

/// Metadata about the embedding model in use at startup. `dimensions` selects
/// the per-dimension chunk table (e.g. `document_chunks_768`) so swapping
/// models with different vector lengths does not collide. The value is
/// resolved at startup via [`lookup_dimensions`] (or by probing the model and
/// then [`record_dimensions`]) — it's no longer declared in config.
#[derive(Debug, Clone)]
pub struct EmbeddingModelInfo {
    pub name: String,
    pub dimensions: usize,
}

/// Look up the cached vector length for `model`. `Ok(None)` means we've never
/// seen this model before and the caller should probe it.
pub async fn lookup_dimensions(conn: &Connection, model: &str) -> Result<Option<usize>, DbError> {
    let mut rows = conn
        .query(
            "SELECT dimensions FROM embedding_model_dimensions WHERE model = ?",
            (model.to_string(),),
        )
        .await
        .map_err(|source| DbError::Query {
            op: "lookup_dimensions",
            source,
        })?;
    match rows.next().await.map_err(|source| DbError::Query {
        op: "lookup_dimensions.next",
        source,
    })? {
        Some(row) => {
            let dims: i64 = row.get(0).map_err(|source| DbError::Query {
                op: "lookup_dimensions.get",
                source,
            })?;
            Ok(Some(dims as usize))
        }
        None => Ok(None),
    }
}

/// Persist a probed vector length. `INSERT OR IGNORE` keeps two racing
/// processes from each writing a row for the same model.
pub async fn record_dimensions(
    conn: &Connection,
    model: &str,
    dimensions: usize,
) -> Result<(), DbError> {
    conn.execute(
        "INSERT OR IGNORE INTO embedding_model_dimensions (model, dimensions) VALUES (?, ?)",
        turso::params::Params::Positional(vec![
            Value::Text(model.to_string()),
            Value::Integer(dimensions as i64),
        ]),
    )
    .await
    .map_err(|source| DbError::Query {
        op: "record_dimensions",
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
