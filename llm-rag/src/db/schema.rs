use turso::Connection;

use crate::config::{Config, EmbeddingsProviderConfig};

use super::error::DbError;

/// Metadata about the embedding model in use at startup. `dimensions` selects
/// the per-dimension chunk table (e.g. `document_chunks_768`) so swapping
/// models with different vector lengths does not collide.
#[derive(Debug, Clone)]
pub struct EmbeddingModelInfo {
    pub name: String,
    pub dimensions: usize,
}

impl EmbeddingModelInfo {
    /// Derive the active embedding model from the loaded [`Config`]. Today
    /// only Ollama is supported and its `dimensions` are declared in the
    /// config file (not queried from the model), which matches how
    /// `crate::llm::build` uses them.
    pub fn from_config(cfg: &Config) -> Self {
        match &cfg.llm.embeddings {
            EmbeddingsProviderConfig::Ollama {
                model, dimensions, ..
            } => Self {
                name: model.clone(),
                dimensions: *dimensions,
            },
        }
    }
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
