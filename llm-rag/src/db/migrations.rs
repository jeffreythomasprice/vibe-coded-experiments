//! Tiny embedded migration runner.
//!
//! Every migration is a `.sql` file compiled into the binary via
//! [`include_str!`]. Applied migrations are recorded in `_schema_migrations`
//! so a second run is idempotent. Failure inside a migration rolls back the
//! full file — nothing from a partially-applied migration survives to
//! confuse the next run.

use turso::Connection;

use super::error::DbError;

/// `(version, name, sql)` — `version` must be monotonically increasing.
const MIGRATIONS: &[(u32, &str, &str)] = &[
    (1, "initial", include_str!("migrations/0001_initial.sql")),
    (
        2,
        "embedding_model_dimensions",
        include_str!("migrations/0002_embedding_model_dimensions.sql"),
    ),
    (
        3,
        "conversations",
        include_str!("migrations/0003_conversations.sql"),
    ),
];

/// Ensure `_schema_migrations` exists, then apply any migrations whose
/// `version` isn't already recorded.
pub async fn run(conn: &Connection) -> Result<(), DbError> {
    ensure_bookkeeping(conn).await?;
    let applied = applied_versions(conn).await?;

    for (version, name, sql) in MIGRATIONS {
        if applied.contains(version) {
            continue;
        }
        apply_one(conn, *version, name, sql).await?;
        tracing::info!(version, name, "applied migration");
    }
    Ok(())
}

async fn ensure_bookkeeping(conn: &Connection) -> Result<(), DbError> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS _schema_migrations (
            version    INTEGER PRIMARY KEY,
            name       TEXT    NOT NULL,
            applied_at TEXT    NOT NULL DEFAULT (datetime('now'))
        )",
        (),
    )
    .await
    .map_err(|source| DbError::Query {
        op: "migrations.ensure_bookkeeping",
        source,
    })?;
    Ok(())
}

async fn applied_versions(conn: &Connection) -> Result<Vec<u32>, DbError> {
    let mut rows = conn
        .query("SELECT version FROM _schema_migrations", ())
        .await
        .map_err(|source| DbError::Query {
            op: "migrations.select_applied",
            source,
        })?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await.map_err(|source| DbError::Query {
        op: "migrations.select_applied.next",
        source,
    })? {
        let v: i64 = row.get(0).map_err(|source| DbError::Query {
            op: "migrations.select_applied.get",
            source,
        })?;
        out.push(v as u32);
    }
    Ok(out)
}

async fn apply_one(
    conn: &Connection,
    version: u32,
    name: &'static str,
    sql: &str,
) -> Result<(), DbError> {
    // turso doesn't expose a savepoint/transaction struct that can wrap async
    // code cleanly, so we run BEGIN/COMMIT as plain statements. On any error
    // inside the migration we ROLLBACK and surface DbError::Migrate.
    conn.execute("BEGIN", ())
        .await
        .map_err(|source| DbError::Migrate {
            version,
            name,
            source,
        })?;

    if let Err(err) = apply_statements(conn, sql).await {
        // Best-effort rollback; if it fails, return the original error.
        let _ = conn.execute("ROLLBACK", ()).await;
        return Err(err.into_migrate(version, name));
    }

    let insert = format!(
        "INSERT INTO _schema_migrations (version, name) VALUES ({version}, '{}')",
        // name is a compile-time &'static str sourced from this file; no
        // untrusted input reaches this format!.
        name.replace('\'', "''")
    );
    if let Err(source) = conn.execute(&insert, ()).await {
        let _ = conn.execute("ROLLBACK", ()).await;
        return Err(DbError::Migrate {
            version,
            name,
            source,
        });
    }

    conn.execute("COMMIT", ())
        .await
        .map_err(|source| DbError::Migrate {
            version,
            name,
            source,
        })?;
    Ok(())
}

/// Error wrapper used while applying a migration. Lets us return either a
/// turso error (from the SQL) or a propagated `DbError` and then tag both
/// with the migration's (version, name) in one place.
enum ApplyError {
    Turso(turso::Error),
    Db(DbError),
}

impl ApplyError {
    fn into_migrate(self, version: u32, name: &'static str) -> DbError {
        match self {
            ApplyError::Turso(source) => DbError::Migrate {
                version,
                name,
                source,
            },
            ApplyError::Db(db) => db,
        }
    }
}

impl From<turso::Error> for ApplyError {
    fn from(value: turso::Error) -> Self {
        ApplyError::Turso(value)
    }
}

impl From<DbError> for ApplyError {
    fn from(value: DbError) -> Self {
        ApplyError::Db(value)
    }
}

async fn apply_statements(conn: &Connection, sql: &str) -> Result<(), ApplyError> {
    for stmt in split_sql(sql) {
        conn.execute(&stmt, ()).await?;
    }
    Ok(())
}

/// Naive `;`-based splitter that respects single-quoted string literals and
/// skips line comments introduced by `--`. Good enough for the hand-written
/// migration SQL in this repo; intentionally not a full SQL parser.
fn split_sql(sql: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut in_line_comment = false;
    let mut chars = sql.chars().peekable();

    while let Some(c) = chars.next() {
        if in_line_comment {
            if c == '\n' {
                in_line_comment = false;
                current.push(c);
            }
            continue;
        }
        if in_string {
            current.push(c);
            if c == '\'' {
                // `''` is a literal quote inside a string; stay in string mode.
                if chars.peek() == Some(&'\'') {
                    current.push(chars.next().unwrap());
                } else {
                    in_string = false;
                }
            }
            continue;
        }
        match c {
            '\'' => {
                in_string = true;
                current.push(c);
            }
            '-' if chars.peek() == Some(&'-') => {
                in_line_comment = true;
                chars.next();
            }
            ';' => {
                let stmt = current.trim().to_string();
                if !stmt.is_empty() {
                    out.push(stmt);
                }
                current.clear();
            }
            _ => current.push(c),
        }
    }

    let tail = current.trim().to_string();
    if !tail.is_empty() {
        out.push(tail);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use turso::Builder;

    async fn mem_conn() -> Connection {
        let db = Builder::new_local(":memory:").build().await.unwrap();
        db.connect().unwrap()
    }

    #[test]
    fn split_sql_handles_basic_statements() {
        let sql = "CREATE TABLE a (id INTEGER);\n\
                   CREATE TABLE b (id INTEGER);";
        let parts = split_sql(sql);
        assert_eq!(parts.len(), 2);
        assert!(parts[0].starts_with("CREATE TABLE a"));
        assert!(parts[1].starts_with("CREATE TABLE b"));
    }

    #[test]
    fn split_sql_ignores_semicolons_inside_strings() {
        let sql = "INSERT INTO t VALUES ('a;b'); CREATE TABLE c (x INT);";
        let parts = split_sql(sql);
        assert_eq!(parts.len(), 2);
        assert!(parts[0].contains("'a;b'"));
    }

    #[test]
    fn split_sql_strips_line_comments() {
        let sql = "-- leading comment\nCREATE TABLE t (id INTEGER); -- trailing\n";
        let parts = split_sql(sql);
        assert_eq!(parts.len(), 1);
        assert!(parts[0].starts_with("CREATE TABLE"));
    }

    #[test]
    fn split_sql_preserves_escaped_quotes() {
        let sql = "INSERT INTO t VALUES ('it''s; fine');";
        let parts = split_sql(sql);
        assert_eq!(parts.len(), 1);
        assert!(parts[0].contains("'it''s; fine'"));
    }

    #[tokio::test]
    async fn run_applies_initial_migration() {
        let conn = mem_conn().await;
        run(&conn).await.expect("migrations should apply");

        let mut rows = conn
            .query(
                "SELECT version, name FROM _schema_migrations ORDER BY version",
                (),
            )
            .await
            .unwrap();
        let mut applied = Vec::new();
        while let Some(row) = rows.next().await.unwrap() {
            let version: i64 = row.get(0).unwrap();
            let name: String = row.get(1).unwrap();
            applied.push((version, name));
        }
        let expected: Vec<(i64, String)> = MIGRATIONS
            .iter()
            .map(|(v, n, _)| (*v as i64, (*n).to_string()))
            .collect();
        assert_eq!(applied, expected);

        // Sanity: the target tables are queryable.
        conn.execute("INSERT INTO documents (path) VALUES ('x')", ())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn run_is_idempotent() {
        let conn = mem_conn().await;
        run(&conn).await.unwrap();
        run(&conn).await.expect("re-run should be a no-op");

        let mut rows = conn
            .query("SELECT count(*) FROM _schema_migrations", ())
            .await
            .unwrap();
        let row = rows.next().await.unwrap().unwrap();
        let count: i64 = row.get(0).unwrap();
        assert_eq!(count, MIGRATIONS.len() as i64);
    }

    #[tokio::test]
    async fn failed_migration_rolls_back() {
        let conn = mem_conn().await;
        ensure_bookkeeping(&conn).await.unwrap();
        // Feed a migration whose SQL will fail partway through. The first
        // statement succeeds; the second references an undefined column.
        let broken = "CREATE TABLE x (id INTEGER);\nINSERT INTO x (nope) VALUES (1);";
        let err = apply_one(&conn, 99, "broken", broken).await.unwrap_err();
        assert!(matches!(err, DbError::Migrate { version: 99, .. }));

        // The bookkeeping row must not exist.
        let mut rows = conn
            .query(
                "SELECT count(*) FROM _schema_migrations WHERE version = 99",
                (),
            )
            .await
            .unwrap();
        let row = rows.next().await.unwrap().unwrap();
        let count: i64 = row.get(0).unwrap();
        assert_eq!(count, 0);
    }
}
