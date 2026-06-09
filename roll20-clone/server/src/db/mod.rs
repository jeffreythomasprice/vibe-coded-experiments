mod migrations;

use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use turso::{Builder, Database};

/// Open the local Turso/libSQL database at `path`, ensuring its parent directory
/// exists, then run any pending migrations.
///
/// Returns the shared `Database`; callers obtain a `Connection` via
/// `db.connect()` when they need to query (connecting is cheap).
pub async fn open(path: &Path) -> anyhow::Result<Arc<Database>> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating database directory {}", parent.display()))?;
    }

    let db = Builder::new_local(path.to_string_lossy().as_ref())
        .build()
        .await
        .with_context(|| format!("opening database {}", path.display()))?;

    let conn = db.connect().context("connecting to database")?;
    migrations::run(&conn).await.context("running migrations")?;

    Ok(Arc::new(db))
}
