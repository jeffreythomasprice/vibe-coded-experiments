//! A hand-rolled migration runner: an ordered list of migrations, each a
//! plain SQL script or a Rust function, applied in one `BEGIN IMMEDIATE`
//! transaction per migration alongside its own ledger row — so a failure
//! rolls back both the schema change and the record of it, and the next
//! start retries exactly that migration.
//!
//! `sqlx::migrate!` was deliberately not used: it only runs SQL, and a
//! backfill that has to read a row, run it through `serde_json` and
//! `shared`'s types, and write it back needs Rust.

use futures_util::future::BoxFuture;
use sqlx::sqlite::SqliteConnection;
use sqlx::{Row, SqlitePool};

use super::error::DbError;
use super::Result;

/// A migration that needs Rust rather than plain SQL — anything that has to
/// read a row, transform it in Rust, and write it back. Write it as:
///
/// ```ignore
/// fn my_step(conn: &mut SqliteConnection) -> BoxFuture<'_, anyhow::Result<()>> {
///     Box::pin(async move { /* ... */ Ok(()) })
/// }
/// ```
///
/// (An `async fn` can't be named in a `const`, hence the explicit
/// `BoxFuture` return.) Runs on the same transaction the ledger insert goes
/// in, so its writes and that insert commit or roll back together.
pub type RustStep = for<'c> fn(&'c mut SqliteConnection) -> BoxFuture<'c, anyhow::Result<()>>;

pub enum Step {
    /// One or more statements, separated by `;`, run through
    /// [`sqlx::raw_sql`] so a single migration file can hold a whole schema.
    Sql(&'static str),
    Rust(RustStep),
}

pub struct Migration {
    pub version: i64,
    /// Recorded in the `_migrations` ledger and re-checked on every start
    /// against what's already recorded there. A mismatch at an
    /// already-applied version means the compiled list was reordered or an
    /// applied migration's name was edited — a hard error, either of which
    /// would otherwise silently skip a schema change.
    pub name: &'static str,
    pub step: Step,
}

pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "init",
        step: Step::Sql(include_str!("sql/0001_init.sql")),
    },
    Migration {
        version: 2,
        name: "preferences",
        step: Step::Sql(include_str!("sql/0002_preferences.sql")),
    },
];

const LEDGER_SQL: &str = "CREATE TABLE IF NOT EXISTS _migrations (\n\
    version INTEGER PRIMARY KEY,\n\
    name TEXT NOT NULL,\n\
    applied_at TEXT NOT NULL\n\
) STRICT;";

/// Apply every migration in `migrations` not already recorded in
/// `_migrations`, in `version` order. Takes the list explicitly (rather than
/// always using [`MIGRATIONS`]) so tests can exercise the runner against
/// their own migrations, including a [`Step::Rust`] one.
///
/// Returns the versions newly applied, oldest first.
pub async fn run(pool: &SqlitePool, migrations: &[Migration]) -> Result<Vec<i64>> {
    sqlx::raw_sql(LEDGER_SQL).execute(pool).await?;

    let recorded: Vec<(i64, String)> = sqlx::query("SELECT version, name FROM _migrations")
        .try_map(|row: sqlx::sqlite::SqliteRow| {
            Ok((row.try_get::<i64, _>("version")?, row.try_get::<String, _>("name")?))
        })
        .fetch_all(pool)
        .await?;
    let recorded: std::collections::HashMap<i64, String> = recorded.into_iter().collect();

    let mut applied = Vec::new();
    for migration in migrations {
        if let Some(name) = recorded.get(&migration.version) {
            if name != migration.name {
                return Err(DbError::MigrationNameMismatch {
                    version: migration.version,
                    recorded: name.clone(),
                    expected: migration.name,
                });
            }
            continue;
        }

        let mut tx = super::write_tx(pool).await?;
        let step_result: anyhow::Result<()> = async {
            match &migration.step {
                Step::Sql(sql) => {
                    sqlx::raw_sql(*sql).execute(&mut *tx).await?;
                }
                Step::Rust(step) => {
                    step(&mut tx).await?;
                }
            }
            Ok(())
        }
        .await;
        step_result.map_err(|source| DbError::Migration {
            version: migration.version,
            name: migration.name,
            source,
        })?;

        sqlx::query("INSERT INTO _migrations (version, name, applied_at) VALUES (?1, ?2, ?3)")
            .bind(migration.version)
            .bind(migration.name)
            .bind(super::now_iso8601())
            .execute(&mut *tx)
            .await
            .map_err(|source| DbError::Migration {
                version: migration.version,
                name: migration.name,
                source: source.into(),
            })?;

        tx.commit().await.map_err(|source| DbError::Migration {
            version: migration.version,
            name: migration.name,
            source: source.into(),
        })?;
        applied.push(migration.version);
    }
    Ok(applied)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqliteConnectOptions;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn fresh_pool() -> SqlitePool {
        let options = SqliteConnectOptions::new().filename(":memory:");
        SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap()
    }

    fn sql_step(version: i64, name: &'static str, sql: &'static str) -> Migration {
        Migration {
            version,
            name,
            step: Step::Sql(sql),
        }
    }

    #[tokio::test]
    async fn a_fresh_database_applies_every_migration() {
        let pool = fresh_pool().await;
        let applied = run(&pool, MIGRATIONS).await.unwrap();
        assert_eq!(applied, vec![1, 2]);

        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM _migrations")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, MIGRATIONS.len() as i64);
    }

    #[tokio::test]
    async fn running_twice_applies_nothing_the_second_time() {
        let pool = fresh_pool().await;
        run(&pool, MIGRATIONS).await.unwrap();
        let applied = run(&pool, MIGRATIONS).await.unwrap();
        assert!(applied.is_empty());
    }

    #[tokio::test]
    async fn a_rust_step_sees_the_table_its_predecessor_sql_step_created_and_commits_with_the_ledger() {
        fn backfill(conn: &mut SqliteConnection) -> BoxFuture<'_, anyhow::Result<()>> {
            Box::pin(async move {
                sqlx::query("INSERT INTO widgets (name) VALUES ('backfilled')")
                    .execute(&mut *conn)
                    .await?;
                Ok(())
            })
        }

        let migrations = vec![
            sql_step(
                1,
                "widgets",
                "CREATE TABLE widgets (id INTEGER PRIMARY KEY, name TEXT NOT NULL) STRICT;",
            ),
            Migration {
                version: 2,
                name: "backfill",
                step: Step::Rust(backfill),
            },
        ];

        let pool = fresh_pool().await;
        let applied = run(&pool, &migrations).await.unwrap();
        assert_eq!(applied, vec![1, 2]);

        let name: String = sqlx::query_scalar("SELECT name FROM widgets")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(name, "backfilled");

        let ledger_count: i64 = sqlx::query_scalar("SELECT count(*) FROM _migrations WHERE version = 2")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(ledger_count, 1);
    }

    #[tokio::test]
    async fn a_failing_migration_rolls_back_both_its_schema_change_and_its_ledger_row_and_later_migrations_do_not_run(
    ) {
        let migrations = vec![
            sql_step(
                1,
                "widgets",
                "CREATE TABLE widgets (id INTEGER PRIMARY KEY) STRICT;",
            ),
            sql_step(2, "broken", "this is not valid sql;"),
            sql_step(
                3,
                "gadgets",
                "CREATE TABLE gadgets (id INTEGER PRIMARY KEY) STRICT;",
            ),
        ];

        let pool = fresh_pool().await;
        let err = run(&pool, &migrations).await.unwrap_err();
        assert!(matches!(err, DbError::Migration { version: 2, .. }), "{err:?}");

        // Migration 1 committed.
        let widgets: i64 = sqlx::query_scalar("SELECT count(*) FROM _migrations WHERE version = 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(widgets, 1);

        // Migration 2 left no ledger row...
        let broken: i64 = sqlx::query_scalar("SELECT count(*) FROM _migrations WHERE version = 2")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(broken, 0);

        // ...and migration 3 never ran.
        let gadgets_table: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'gadgets'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(gadgets_table, 0);
    }

    #[tokio::test]
    async fn a_renamed_already_applied_migration_is_a_hard_error() {
        let pool = fresh_pool().await;
        run(&pool, &[sql_step(1, "widgets", "CREATE TABLE widgets (id INTEGER PRIMARY KEY) STRICT;")])
            .await
            .unwrap();

        let renamed = [sql_step(
            1,
            "renamed",
            "CREATE TABLE widgets (id INTEGER PRIMARY KEY) STRICT;",
        )];
        let err = run(&pool, &renamed).await.unwrap_err();
        assert!(
            matches!(
                &err,
                DbError::MigrationNameMismatch { version: 1, recorded, expected }
                    if recorded == "widgets" && *expected == "renamed"
            ),
            "{err:?}"
        );
    }
}
