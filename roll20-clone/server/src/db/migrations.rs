use anyhow::{anyhow, Context};
use turso::{Connection, Value};

/// A single forward migration. `version` is a stable identifier recorded in the
/// `schema_migrations` table once applied; `sql` may contain multiple
/// `;`-separated statements.
struct Migration {
    version: &'static str,
    sql: &'static str,
}

/// All migrations, in apply order. Append new entries; never edit or reorder
/// past ones.
const MIGRATIONS: &[Migration] = &[Migration {
    version: "0001_init",
    sql: include_str!("../../migrations/0001_init.sql"),
}];

/// Ensure the bookkeeping table exists, then apply every migration that hasn't
/// been recorded yet.
pub async fn run(conn: &Connection) -> anyhow::Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version    TEXT PRIMARY KEY,
            applied_at INTEGER NOT NULL
        )",
        (),
    )
    .await
    .context("creating schema_migrations table")?;

    for migration in MIGRATIONS {
        if is_applied(conn, migration.version).await? {
            continue;
        }

        for statement in split_statements(migration.sql) {
            conn.execute(&statement, ())
                .await
                .with_context(|| format!("applying migration {}", migration.version))?;
        }

        conn.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, strftime('%s', 'now'))",
            [migration.version],
        )
        .await
        .with_context(|| format!("recording migration {}", migration.version))?;

        tracing::info!(version = migration.version, "applied migration");
    }

    Ok(())
}

/// Whether a migration `version` has already been recorded.
async fn is_applied(conn: &Connection, version: &str) -> anyhow::Result<bool> {
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM schema_migrations WHERE version = ?1",
            [version],
        )
        .await?;
    let row = rows
        .next()
        .await?
        .ok_or_else(|| anyhow!("COUNT(*) returned no row"))?;
    match row.get_value(0)? {
        Value::Integer(n) => Ok(n > 0),
        other => Err(anyhow!("unexpected COUNT(*) value: {other:?}")),
    }
}

/// Split a SQL string into individual statements on `;`, after stripping `--`
/// line comments and dropping blank fragments. (Sufficient for our migrations,
/// which don't contain `--` or `;` inside string literals.)
fn split_statements(sql: &str) -> Vec<String> {
    let without_comments = sql
        .lines()
        .map(|line| match line.find("--") {
            Some(i) => &line[..i],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n");

    without_comments
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}
