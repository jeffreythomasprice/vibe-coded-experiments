//! turso-backed storage: connection setup, migrations, embedding-dim
//! resolution, and per-dimension chunks table creation.
#![allow(dead_code)]

pub mod migrations;
pub mod vector;

use std::path::PathBuf;

use turso::{Builder, Connection, Database, Value};

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
///
/// `database` is retained so callers that need to read concurrently with the
/// worker's open transaction can mint a separate `Connection` via
/// [`Db::fresh_conn`].
pub struct Db {
    pub database: Database,
    pub conn: Connection,
    pub embedding_model: String,
    pub embedding_dimensions: usize,
    pub chunks_table: String,
    pub summary_vectors_table: String,
}

impl Db {
    /// Open a fresh `Connection` against the same underlying `Database` and
    /// enable foreign keys on it. Use this when you need DB access from a
    /// task that must not interleave with whatever transaction is open on
    /// `self.conn`. Turso 0.4's `BEGIN`/`COMMIT` are connection-scoped, so a
    /// SELECT on the worker's `conn` while it's mid-ingest would land inside
    /// the ingest transaction.
    pub async fn fresh_conn(&self) -> Result<Connection, DbError> {
        let conn = self.database.connect().map_err(|source| DbError::Open {
            path: PathBuf::new(),
            source,
        })?;
        conn.execute("PRAGMA foreign_keys = ON", ())
            .await
            .map_err(|source| DbError::Query {
                op: "fresh_conn.pragma.foreign_keys",
                source,
            })?;
        Ok(conn)
    }

    /// Insert an `in-progress` row for a task the worker has just started.
    pub async fn task_log_start(
        &self,
        job_id: &str,
        task_name: &str,
        label: &str,
        path: Option<&str>,
        tags: Option<&str>,
    ) -> Result<(), DbError> {
        self.conn
            .execute(
                "INSERT INTO task_log (job_id, task_name, label, path, tags, status) \
                 VALUES (?, ?, ?, ?, ?, 'in-progress')",
                turso::params::Params::Positional(vec![
                    Value::Text(job_id.to_string()),
                    Value::Text(task_name.to_string()),
                    Value::Text(label.to_string()),
                    match path {
                        Some(p) => Value::Text(p.to_string()),
                        None => Value::Null,
                    },
                    match tags {
                        Some(t) => Value::Text(t.to_string()),
                        None => Value::Null,
                    },
                ]),
            )
            .await
            .map_err(|source| DbError::Query {
                op: "task_log_start",
                source,
            })?;
        Ok(())
    }

    /// Update a task_log row to its terminal state.
    pub async fn task_log_finish(
        &self,
        job_id: &str,
        status: &str,
        error: Option<&str>,
    ) -> Result<(), DbError> {
        self.conn
            .execute(
                "UPDATE task_log \
                    SET status = ?, error = ?, ended_at = datetime('now') \
                  WHERE job_id = ?",
                turso::params::Params::Positional(vec![
                    Value::Text(status.to_string()),
                    match error {
                        Some(e) => Value::Text(e.to_string()),
                        None => Value::Null,
                    },
                    Value::Text(job_id.to_string()),
                ]),
            )
            .await
            .map_err(|source| DbError::Query {
                op: "task_log_finish",
                source,
            })?;
        Ok(())
    }

    /// Mark any leftover `in-progress` rows as `failure` with an explanation.
    /// Called once at server start so a previously crashed/killed run doesn't
    /// leave rows stuck.
    pub async fn task_log_repair_abandoned(&self) -> Result<(), DbError> {
        self.conn
            .execute(
                "UPDATE task_log \
                    SET status = 'failure', \
                        ended_at = datetime('now'), \
                        error = COALESCE(error, 'abandoned (server restart)') \
                  WHERE status = 'in-progress'",
                (),
            )
            .await
            .map_err(|source| DbError::Query {
                op: "task_log_repair_abandoned",
                source,
            })?;
        Ok(())
    }
}

/// One row from the `task_log` table, as returned to the client.
#[derive(Debug, Clone)]
pub struct TaskLogRow {
    pub job_id: String,
    pub task_name: String,
    pub label: String,
    pub path: Option<String>,
    pub tags: Option<String>,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub status: String,
    pub error: Option<String>,
}

/// Fetch the most-recent `limit` task_log rows in chronological order
/// (oldest first, newest last). Uses a `fresh_conn` since this is called
/// from the inline handler concurrently with the worker.
pub async fn task_log_recent(db: &Db, limit: usize) -> Result<Vec<TaskLogRow>, DbError> {
    let conn = db.fresh_conn().await?;
    let mut rows = conn
        .query(
            "SELECT job_id, task_name, label, path, tags, started_at, ended_at, status, error \
               FROM task_log \
              ORDER BY started_at DESC, id DESC \
              LIMIT ?",
            (limit as i64,),
        )
        .await
        .map_err(|source| DbError::Query {
            op: "task_log_recent",
            source,
        })?;
    let mut out: Vec<TaskLogRow> = Vec::new();
    while let Some(row) = rows.next().await.map_err(|source| DbError::Query {
        op: "task_log_recent.next",
        source,
    })? {
        let job_id: String = row.get(0).map_err(|source| DbError::Query {
            op: "task_log_recent.job_id",
            source,
        })?;
        let task_name: String = row.get(1).map_err(|source| DbError::Query {
            op: "task_log_recent.task_name",
            source,
        })?;
        let label: String = row.get(2).map_err(|source| DbError::Query {
            op: "task_log_recent.label",
            source,
        })?;
        let path = nullable_text(&row, 3, "task_log_recent.path")?;
        let tags = nullable_text(&row, 4, "task_log_recent.tags")?;
        let started_at: String = row.get(5).map_err(|source| DbError::Query {
            op: "task_log_recent.started_at",
            source,
        })?;
        let ended_at = nullable_text(&row, 6, "task_log_recent.ended_at")?;
        let status: String = row.get(7).map_err(|source| DbError::Query {
            op: "task_log_recent.status",
            source,
        })?;
        let error = nullable_text(&row, 8, "task_log_recent.error")?;
        out.push(TaskLogRow {
            job_id,
            task_name,
            label,
            path,
            tags,
            started_at,
            ended_at,
            status,
            error,
        });
    }
    // Reverse so the oldest of the N is first and the newest is last.
    out.reverse();
    Ok(out)
}

fn nullable_text(
    row: &turso::Row,
    idx: usize,
    op: &'static str,
) -> Result<Option<String>, DbError> {
    match row.get_value(idx) {
        Ok(Value::Null) => Ok(None),
        Ok(Value::Text(s)) => Ok(Some(s)),
        Ok(other) => panic!("unexpected non-text value for {op}: {other:?}"),
        Err(source) => Err(DbError::Query { op, source }),
    }
}

/// Returns the SQL table name for chunks of a given embedding dimension.
pub fn chunks_table_name(dims: usize) -> String {
    format!("chunks_{dims}")
}

/// Returns the SQL table name for summary embeddings of a given dimension.
/// Parallels [`chunks_table_name`] — same per-dimension table-per-model
/// pattern, with summaries keyed by `document_summary.id` instead of
/// `document_chunk.id`.
pub fn summary_vectors_table_name(dims: usize) -> String {
    format!("summary_vectors_{dims}")
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

    let database = Builder::new_local(&path_str)
        .build()
        .await
        .map_err(|source| DbError::Open {
            path: cfg.db_path.clone(),
            source,
        })?;
    let conn = database.connect().map_err(|source| DbError::Open {
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

    let summary_vectors_table = summary_vectors_table_name(dimensions);
    ensure_summary_vectors_table(&conn, &summary_vectors_table, dimensions).await?;

    Ok(Db {
        database,
        conn,
        embedding_model: model,
        embedding_dimensions: dimensions,
        chunks_table,
        summary_vectors_table,
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

/// Idempotently create the per-dimension summary embeddings table. Same shape
/// as the chunks table but keyed by `document_summary.id`.
async fn ensure_summary_vectors_table(
    conn: &Connection,
    table: &str,
    dims: usize,
) -> Result<(), DbError> {
    let create_table = format!(
        "CREATE TABLE IF NOT EXISTS {table} (
            summary_id INTEGER PRIMARY KEY REFERENCES document_summary(id) ON DELETE CASCADE,
            embedding  F32_BLOB({dims}) NOT NULL
        )"
    );
    conn.execute(&create_table, ())
        .await
        .map_err(|source| DbError::Query {
            op: "ensure_summary_vectors_table.create",
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

    #[test]
    fn summary_vectors_table_name_interpolates_dims() {
        assert_eq!(summary_vectors_table_name(768), "summary_vectors_768");
        assert_eq!(summary_vectors_table_name(1536), "summary_vectors_1536");
    }
}
