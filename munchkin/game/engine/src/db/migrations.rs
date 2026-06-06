//! Hand-rolled, forward-only migration runner. Each migration is an embedded
//! `.sql` file with a monotonically increasing version. Applied versions are
//! recorded in `_migrations`; on every startup only pending migrations are
//! applied, in order. Safe to run repeatedly.

use anyhow::{Context, Result};
use turso::Connection;

struct Migration {
    version: i64,
    name: &'static str,
    sql: &'static str,
}

/// Ordered by `version`. Add new entries; never edit or renumber applied ones.
const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    name: "initial",
    sql: include_str!("migrations/0001_initial.sql"),
}];

pub async fn run(conn: &Connection) -> Result<()> {
    exec_script(
        conn,
        "CREATE TABLE IF NOT EXISTS _migrations (
             version    INTEGER PRIMARY KEY,
             name       TEXT NOT NULL,
             applied_at TEXT NOT NULL DEFAULT (datetime('now'))
         );",
    )
    .await
    .context("ensuring _migrations table")?;

    let current = current_version(conn).await?;

    for m in MIGRATIONS {
        if m.version <= current {
            continue;
        }
        tracing::info!(version = m.version, name = m.name, "applying migration");
        exec_script(conn, m.sql)
            .await
            .with_context(|| format!("applying migration {} ({})", m.version, m.name))?;
        conn.execute(
            "INSERT INTO _migrations (version, name) VALUES (?1, ?2)",
            (m.version, m.name),
        )
        .await
        .with_context(|| format!("recording migration {}", m.version))?;
    }

    tracing::debug!(
        applied_through = MIGRATIONS.last().map_or(0, |m| m.version),
        "migrations up to date"
    );
    Ok(())
}

/// Execute a `.sql` script one statement at a time (turso has no execute_batch).
/// Strips `--` line comments, then splits on `;`. Naive — fine for our simple,
/// hand-written migrations; revisit if a migration ever needs `--` or `;` inside
/// a string literal (or a trigger body). A comment-only script runs nothing,
/// which is how the placeholder migration is a true no-op. Statements must not
/// return rows (use plain DDL/DML; turso's `execute` rejects result rows).
async fn exec_script(conn: &Connection, sql: &str) -> Result<()> {
    let stripped: String = sql
        .lines()
        .map(|line| line.split_once("--").map_or(line, |(code, _)| code))
        .collect::<Vec<_>>()
        .join("\n");
    for stmt in stripped.split(';') {
        let stmt = stmt.trim();
        if stmt.is_empty() {
            continue;
        }
        conn.execute(stmt, ())
            .await
            .with_context(|| format!("executing statement: {stmt}"))?;
    }
    Ok(())
}

async fn current_version(conn: &Connection) -> Result<i64> {
    let mut stmt = conn
        .prepare("SELECT COALESCE(MAX(version), 0) FROM _migrations")
        .await
        .context("preparing version query")?;
    let mut rows = stmt
        .query(())
        .await
        .context("reading current migration version")?;
    let row = rows
        .next()
        .await
        .context("reading migration version row")?
        .context("MAX(version) returned no row")?;
    row.get::<i64>(0).context("decoding migration version")
}
