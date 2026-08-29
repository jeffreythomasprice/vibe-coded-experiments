//! The local SQLite-backed store: agent configs, conversations, turns,
//! messages, and their `tool_calls` projection.
//!
//! ## Invariants every writer here must preserve
//!
//! - **`messages` always holds a sendable conversation prefix.** Every
//!   `tool_use` block in it has a matching `tool_result` in the very next
//!   message. A turn's `added` messages are therefore held back — never
//!   written to `messages` — until the turn reaches a terminal outcome.
//!   `Done`/`MaxSteps`/`AwaitingApproval` all count as terminal here; only a
//!   hard stream failure discards a turn's messages entirely. See
//!   [`turns::begin`] and [`turns::settle`].
//! - **A suspended turn is stored as the whole `AgentTurn`, verbatim.**
//!   `turns.turn_json` holds a `serde_json` round trip of
//!   `shared::agent::AgentTurn` while a turn is `running` or
//!   `awaiting_approval`. `Agent::resume` re-validates the trailing
//!   assistant message's tool-use ids against `pending`/`completed`, and
//!   `steps` must carry forward unchanged (it decides the next
//!   `call_{step}_{ordinal}` ids) — only a byte-exact round trip can promise
//!   both; reconstructing the value from `messages` rows cannot. See
//!   `sql/0001_init.sql`'s doc on `turns`.
//! - **`tool_calls` is a projection, never a source of truth.** It is
//!   rebuilt wholesale per turn by [`turns::rebuild_tool_calls`] from
//!   `messages.content` and `turns.turn_json`. The one fact it alone holds is
//!   `decision` ('approve'/'deny'), because `Agent::resume`'s denial
//!   placeholder is shape-identical on the wire to a tool that failed on its
//!   own — rebuilding must never clobber it.

pub mod agents;
pub mod conversations;
pub mod error;
pub mod migrate;
pub mod preferences;
pub mod projects;
#[cfg(test)]
pub(crate) mod testing;
pub mod themes;
pub mod turns;

pub use error::{DbError, Result};

use std::time::Duration;

use shared::error::{ErrorKind, ErrorReport};
use shared::ids::ConversationId;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::config::DatabaseConfig;

/// Re-exported for every existing call site in this module — the formatting
/// itself lives in `lib::clock` (used by `lib::agent` too, which has no
/// business depending on `lib::db`).
pub use crate::clock::now_iso8601;

/// A single-user desktop file has no deployment tuning story worth exposing —
/// see `DatabaseConfig`'s doc — so pool size and busy timeout are constants,
/// not config keys.
const MAX_CONNECTIONS: u32 = 5;
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone)]
pub struct Db {
    pool: SqlitePool,
}

impl Db {
    /// Open (creating the file and its parent directory if absent), apply
    /// every pending migration, and reap any turn a previous process left
    /// `running` (see [`reap_orphaned`]). Call once, at startup.
    pub async fn open(config: &DatabaseConfig) -> Result<Self> {
        if let Some(parent) = config.path.parent() {
            std::fs::create_dir_all(parent).map_err(sqlx::Error::Io)?;
        }
        let options = SqliteConnectOptions::new()
            .filename(&config.path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(true)
            .busy_timeout(BUSY_TIMEOUT);
        let pool = SqlitePoolOptions::new()
            .max_connections(MAX_CONNECTIONS)
            .connect_with(options)
            .await?;
        let db = Self { pool };
        migrate::run(&db.pool, migrate::MIGRATIONS).await?;
        reap_orphaned(&db.pool, None).await?;
        Ok(db)
    }

    /// In-memory and migrated, for tests that don't need the real file-open
    /// path. A single connection: SQLite's `:memory:` database is private to
    /// the connection that created it, so a pool of more than one would hand
    /// out connections each seeing an empty database.
    #[cfg(test)]
    pub(crate) async fn in_memory() -> Result<Self> {
        let options = SqliteConnectOptions::new()
            .filename(":memory:")
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await?;
        let db = Self { pool };
        migrate::run(&db.pool, migrate::MIGRATIONS).await?;
        Ok(db)
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn close(&self) {
        self.pool.close().await;
    }
}

/// `BEGIN IMMEDIATE`, not the deferred `BEGIN` `Pool::begin` issues. Every
/// write transaction in this module reads before it writes (the current
/// `MAX(seq)`, whether a turn is already open), and a deferred WAL
/// transaction that upgrades to a writer after its snapshot has gone stale
/// fails with `SQLITE_BUSY_SNAPSHOT` — which `busy_timeout` does not retry.
/// Taking the write lock up front turns that into an ordinary, retriable
/// wait instead.
pub(crate) async fn write_tx(pool: &SqlitePool) -> Result<Transaction<'static, Sqlite>> {
    Ok(pool.begin_with("BEGIN IMMEDIATE").await?)
}

/// Fail (or, if it has a resume point, revert to `awaiting_approval`) every
/// turn left `running`.
///
/// A `running` row can only be owned by the live future that is driving its
/// stream, so any row this finds with no such owner is an orphan — from a
/// crash, or from Tauri dropping the command future when the webview went
/// away mid-turn. A row with `turn_json` reverts rather than fails: that is a
/// resume point holding tool results that already ran, and discarding it
/// would silently throw the whole turn away. The tradeoff is that an
/// approved tool whose side effect completed just before the crash may run
/// again when the user approves a second time — a risk
/// `Approval::RequiresApproval` exists to put in front of the user anyway.
///
/// Called with `only: None` once at [`Db::open`] (no live streams exist yet
/// at startup, so every `running` row is dead by construction), and with
/// `only: Some(id)` by `lib::service` right after it acquires a
/// conversation's lock — holding that lock proves no live future owns a
/// `running` row for this conversation either.
pub(crate) async fn reap_orphaned(pool: &SqlitePool, only: Option<ConversationId>) -> Result<u64> {
    let now = now_iso8601();
    let error_json = serde_json::to_string(&ErrorReport {
        kind: ErrorKind::Storage,
        provider: None,
        message: "the app exited while this turn was in progress".to_string(),
        retryable: false,
    })
    .expect("ErrorReport always serializes");

    let result = sqlx::query(
        "UPDATE turns
            SET state = CASE WHEN turn_json IS NOT NULL THEN 'awaiting_approval' ELSE 'failed' END,
                error_json = CASE WHEN turn_json IS NULL THEN ?1 ELSE error_json END,
                finished_at = CASE WHEN turn_json IS NULL THEN ?2 ELSE NULL END
          WHERE state = 'running'
            AND (?3 IS NULL OR conversation_id = ?3)",
    )
    .bind(error_json)
    .bind(now)
    .bind(only.map(ConversationId::get))
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn open_in_memory_runs_migrations() {
        let db = Db::in_memory().await.unwrap();
        // The migrated schema exists and is queryable.
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM agent_configs")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn reap_orphaned_reports_zero_when_nothing_is_running() {
        let db = Db::in_memory().await.unwrap();
        let reaped = reap_orphaned(db.pool(), None).await.unwrap();
        assert_eq!(reaped, 0);
    }
}
