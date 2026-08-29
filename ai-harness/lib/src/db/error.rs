//! Errors [`crate::db`] can produce.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("database: {0}")]
    Sql(#[from] sqlx::Error),

    #[error("migration {version} ({name}) failed: {source}")]
    Migration {
        version: i64,
        name: &'static str,
        source: anyhow::Error,
    },

    /// The `_migrations` ledger names a different migration at `version` than
    /// the compiled [`crate::db::migrate::MIGRATIONS`] list does — someone
    /// reordered or renamed an already-applied entry. A hard error, because
    /// silently trusting either side risks re-running (or never running) a
    /// schema change.
    #[error(
        "migration {version} is recorded in the database as {recorded:?}, \
         but the compiled migration list names it {expected:?}"
    )]
    MigrationNameMismatch {
        version: i64,
        recorded: String,
        expected: &'static str,
    },

    /// A JSON column failed to deserialize back into its `shared` type — the
    /// database file was hand-edited, or written by an incompatible version
    /// of this app.
    #[error("corrupt stored JSON in {table}.{column} for {entity} {id}: {source}")]
    Json {
        table: &'static str,
        column: &'static str,
        entity: &'static str,
        id: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("{entity} {id} not found")]
    NotFound { entity: &'static str, id: String },

    /// A turn is already `running`/`awaiting_approval` for this conversation.
    /// The durable backstop behind `turns_one_open_per_conversation` — see
    /// that index's doc in `sql/0001_init.sql`. `lib::service` catches this
    /// as the common case; `Sql` with a unique-constraint violation is the
    /// fallback if the in-memory lock is ever bypassed.
    #[error("a turn is already in progress for conversation {conversation_id}")]
    ConversationBusy { conversation_id: i64 },

    /// A theme named `name` already exists (case-insensitively), either as
    /// another row in `themes` or as a built-in's label. Detected by a
    /// `SELECT` inside the same `write_tx` as the insert/update, ahead of the
    /// `themes_name` unique index — see `lib::db::themes`'s doc.
    #[error("a theme named {name:?} already exists")]
    ThemeNameTaken { name: String },

    /// A project named `name` already exists (case-insensitively), either as
    /// another row in `projects` or as the default project's reserved name
    /// (`shared::project::DEFAULT_PROJECT_NAME`). Same pattern as
    /// [`DbError::ThemeNameTaken`] — see `lib::db::projects`'s doc.
    #[error("a project named {name:?} already exists")]
    ProjectNameTaken { name: String },
}

pub type Result<T> = std::result::Result<T, DbError>;

impl DbError {
    /// Whether the same operation might succeed if retried as-is — never true
    /// here today. A `Sql` error from a transient SQLite lock contention
    /// (`SQLITE_BUSY`) is possible in principle, but `busy_timeout` already
    /// retries that internally before it ever surfaces; anything that does
    /// surface is either a genuine conflict (see [`DbError::ConversationBusy`])
    /// or a real fault worth surfacing as-is.
    pub fn is_retryable(&self) -> bool {
        false
    }
}

/// Flatten a `DbError` into the same IPC-safe DTO an `LlmError`/`AgentError`
/// becomes. `NotFound` and `ConversationBusy` get their own `ErrorKind` —
/// a UI can tell "this doesn't exist" and "try again in a moment" apart from
/// a raw storage fault; everything else here is `ErrorKind::Storage`, since
/// none of it is something retrying the same call fixes.
impl From<&DbError> for shared::error::ErrorReport {
    fn from(err: &DbError) -> Self {
        use shared::error::{ErrorKind, ErrorReport};
        match err {
            DbError::NotFound { .. } => ErrorReport {
                kind: ErrorKind::NotFound,
                provider: None,
                message: err.to_string(),
                retryable: false,
            },
            DbError::ConversationBusy { .. } => ErrorReport {
                kind: ErrorKind::Conflict,
                provider: None,
                message: err.to_string(),
                retryable: true,
            },
            DbError::ThemeNameTaken { .. } => ErrorReport {
                kind: ErrorKind::Conflict,
                provider: None,
                message: err.to_string(),
                retryable: false,
            },
            DbError::ProjectNameTaken { .. } => ErrorReport {
                kind: ErrorKind::Conflict,
                provider: None,
                message: err.to_string(),
                retryable: false,
            },
            other => ErrorReport {
                kind: ErrorKind::Storage,
                provider: None,
                message: other.to_string(),
                retryable: false,
            },
        }
    }
}
