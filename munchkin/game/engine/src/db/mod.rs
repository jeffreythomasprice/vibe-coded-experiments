//! Engine database: a local `turso` (SQLite-compatible) file plus an embedded,
//! forward-only migration runner. Opened once at startup; the handle (and its
//! tokio runtime) lives for the whole engine process.

use std::path::Path;

use anyhow::{Context, Result};
use tokio::runtime::Runtime;
use turso::{Builder, Connection, Database};

mod migrations;

/// An open database. Owns the tokio runtime so the async connection stays valid
/// for the engine's lifetime; future sync code runs queries via `rt.block_on`.
#[allow(dead_code)] // `rt`/`conn` are unused until the first real query lands.
pub struct Db {
    rt: Runtime,
    db: Database,
    conn: Connection,
}

impl Db {
    /// Open (creating if needed) the database at `path`, creating its parent
    /// directory, and apply any pending migrations. Idempotent.
    pub fn open(path: &Path) -> Result<Db> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("creating db dir {}", parent.display()))?;
            }
        }

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("building tokio runtime")?;

        let path_str = path
            .to_str()
            .with_context(|| format!("db path is not valid UTF-8: {}", path.display()))?
            .to_owned();

        let (db, conn) = rt.block_on(async {
            let db = Builder::new_local(&path_str)
                .build()
                .await
                .with_context(|| format!("opening database {path_str}"))?;
            let conn = db.connect().context("connecting to database")?;
            migrations::run(&conn).await.context("running migrations")?;
            anyhow::Ok((db, conn))
        })?;

        Ok(Db { rt, db, conn })
    }
}
