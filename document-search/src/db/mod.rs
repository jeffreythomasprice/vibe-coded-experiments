//! turso-backed storage: connection setup, migrations, embedding-dim
//! resolution, and per-dimension chunks table creation.
#![allow(dead_code)]

pub mod migrations;
pub mod vector;

use std::path::PathBuf;

use turso::{Builder, Connection, Value};

use crate::config::Config;

#[derive(thiserror::Error, Debug)]
pub enum DbError {
    #[error("opening database at {path}: {source}")]
    Open {
        path: PathBuf,
        #[source]
        source: turso::Error,
    },

    #[error("applying migration {version} ({name}): {source}")]
    Migrate {
        version: u32,
        name: &'static str,
        #[source]
        source: turso::Error,
    },

    #[error("running query ({op}): {source}")]
    Query {
        op: &'static str,
        #[source]
        source: turso::Error,
    },

    #[error("probing embedding model {model:?}: {source}")]
    Probe {
        model: String,
        #[source]
        source: crate::ollama::OllamaError,
    },
}

/// What `open` returns to callers — a live connection plus the resolved
/// embedding metadata so the rest of the app knows which `chunks_<N>` table
/// to write to.
pub struct Db {
    pub conn: Connection,
    pub embedding_model: String,
    pub embedding_dimensions: usize,
    pub chunks_table: String,
}

/// Returns the SQL table name for chunks of a given embedding dimension.
pub fn chunks_table_name(dims: usize) -> String {
    format!("chunks_{dims}")
}

/// Open the configured DB, run migrations, resolve the embedding model's
/// vector length (cached or via a single Ollama probe), and ensure the
/// dimension-specific chunks table exists.
pub async fn open(cfg: &Config, http: &reqwest::Client) -> Result<Db, DbError> {
    let path_str = cfg.db_path.to_string_lossy().to_string();

    if let Some(parent) = cfg.db_path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent).map_err(|e| DbError::Open {
                path: cfg.db_path.clone(),
                source: turso::Error::ConversionFailure(format!(
                    "creating parent dir for db: {e}"
                )),
            })?;
        }
    }

    let db = Builder::new_local(&path_str)
        .build()
        .await
        .map_err(|source| DbError::Open {
            path: cfg.db_path.clone(),
            source,
        })?;
    let conn = db.connect().map_err(|source| DbError::Open {
        path: cfg.db_path.clone(),
        source,
    })?;

    conn.execute("PRAGMA foreign_keys = ON", ())
        .await
        .map_err(|source| DbError::Query {
            op: "pragma.foreign_keys",
            source,
        })?;

    migrations::run(&conn).await?;

    let model = cfg.ollama.embedding_model.clone();
    let dimensions = match lookup_dimensions(&conn, &model).await? {
        Some(dims) => {
            tracing::info!(model = %model, dimensions = dims, "embedding dim resolved from cache");
            dims
        }
        None => {
            tracing::info!(model = %model, "no cached embedding dim; probing ollama");
            let dims = crate::ollama::probe_dimensions(http, &cfg.ollama.url, &model)
                .await
                .map_err(|source| DbError::Probe {
                    model: model.clone(),
                    source,
                })?;
            record_dimensions(&conn, &model, dims).await?;
            tracing::info!(model = %model, dimensions = dims, "embedding dim probed and cached");
            dims
        }
    };

    let chunks_table = chunks_table_name(dimensions);
    ensure_chunks_table(&conn, &chunks_table, dimensions).await?;

    Ok(Db {
        conn,
        embedding_model: model,
        embedding_dimensions: dimensions,
        chunks_table,
    })
}

async fn lookup_dimensions(conn: &Connection, model: &str) -> Result<Option<usize>, DbError> {
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

async fn record_dimensions(conn: &Connection, model: &str, dims: usize) -> Result<(), DbError> {
    conn.execute(
        "INSERT OR IGNORE INTO embedding_model_dimensions (model, dimensions) VALUES (?, ?)",
        turso::params::Params::Positional(vec![
            Value::Text(model.to_string()),
            Value::Integer(dims as i64),
        ]),
    )
    .await
    .map_err(|source| DbError::Query {
        op: "record_dimensions",
        source,
    })?;
    Ok(())
}

/// Idempotently create the per-dimension chunks table. The `embedding`
/// column is `F32_BLOB(N)` so turso's native `vector_distance_cos` works
/// against it. The table is 1:1 with `document_chunk`, keyed by `chunk_id`.
async fn ensure_chunks_table(conn: &Connection, table: &str, dims: usize) -> Result<(), DbError> {
    let create_table = format!(
        "CREATE TABLE IF NOT EXISTS {table} (
            chunk_id  INTEGER PRIMARY KEY REFERENCES document_chunk(id) ON DELETE CASCADE,
            embedding F32_BLOB({dims}) NOT NULL
        )"
    );
    conn.execute(&create_table, ())
        .await
        .map_err(|source| DbError::Query {
            op: "ensure_chunks_table.create",
            source,
        })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_table_name_interpolates_dims() {
        assert_eq!(chunks_table_name(768), "chunks_768");
        assert_eq!(chunks_table_name(1536), "chunks_1536");
    }
}
