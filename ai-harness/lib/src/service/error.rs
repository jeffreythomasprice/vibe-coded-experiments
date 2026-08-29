//! Errors [`crate::service::Service`] can produce.

use thiserror::Error;

use crate::agent::AgentError;
use crate::db::DbError;
use crate::vfs::VfsError;

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error(transparent)]
    Db(#[from] DbError),

    #[error(transparent)]
    Agent(#[from] AgentError),

    /// The in-memory per-conversation lock was already held — a second
    /// `send_message`/`approve_tools` on the same conversation while one is
    /// already streaming. The fast-path counterpart to
    /// [`DbError::ConversationBusy`], which exists as the durable backstop
    /// behind the same guarantee — see `crate::service::chat`'s module doc.
    #[error("conversation {conversation_id} already has a turn in progress")]
    ConversationBusy { conversation_id: i64 },

    /// `approve_tools` was called on a conversation with nothing suspended.
    #[error("conversation {conversation_id} has no turn awaiting approval")]
    NoPendingTurn { conversation_id: i64 },

    /// `Agent::stream_turn`/`resume_stream` ended without ever yielding an
    /// `AgentEvent::Turn` or an `Err` — unreachable given how `lib::agent`'s
    /// loop is written today, but reported as a failure rather than treated
    /// as a silent success if that ever stops being true.
    #[error("the agent's event stream ended without a terminal event")]
    TruncatedStream,

    /// A theme name matches a built-in's label, case-insensitively. The
    /// database's `themes_name` unique index only sees other user themes —
    /// built-ins never have a row — so this half of "a theme's name must be
    /// unique across every theme" is checked here, in
    /// `crate::service::themes`, before ever reaching the database.
    #[error("a theme named {name:?} already exists")]
    ThemeNameConflict { name: String },

    /// A [`shared::theme::ThemeField`]'s value on a `UserThemeInput` isn't
    /// `#rrggbb`.
    #[error("{field} is not a valid color: {value:?} (expected #rrggbb)")]
    InvalidThemeColor { field: &'static str, value: String },

    /// A theme's name was empty or all whitespace.
    #[error("a theme's name can't be empty")]
    EmptyThemeName,

    /// A project name matches the default project's reserved name
    /// (`shared::project::DEFAULT_PROJECT_NAME`), case-insensitively — the
    /// default is never a `projects` row, so the database's unique index
    /// can't see this half of the rule. Same pattern as
    /// [`ServiceError::ThemeNameConflict`].
    #[error("a project named {name:?} already exists")]
    ProjectNameConflict { name: String },

    /// A project's name was empty or all whitespace.
    #[error("a project's name can't be empty")]
    EmptyProjectName,

    /// A project directory failed one of the rules the database can't
    /// express: not absolute, doesn't exist, isn't a directory, or repeats
    /// another entry in the same project.
    #[error("invalid project directory {path:?}: {reason}")]
    InvalidProjectDir { path: String, reason: String },

    /// A project's directories failed to resolve into a `lib::vfs::MountTable`
    /// — surfaces as this rather than a bare `Vfs` variant name so a caller
    /// building a sandboxed run sees why in one place.
    #[error(transparent)]
    Vfs(#[from] VfsError),
}

impl ServiceError {
    pub fn is_retryable(&self) -> bool {
        match self {
            ServiceError::Db(err) => err.is_retryable(),
            ServiceError::Agent(err) => err.is_retryable(),
            ServiceError::ConversationBusy { .. } => true,
            ServiceError::NoPendingTurn { .. } | ServiceError::TruncatedStream => false,
            ServiceError::ThemeNameConflict { .. }
            | ServiceError::InvalidThemeColor { .. }
            | ServiceError::EmptyThemeName => false,
            ServiceError::ProjectNameConflict { .. }
            | ServiceError::EmptyProjectName
            | ServiceError::InvalidProjectDir { .. } => false,
            ServiceError::Vfs(_) => false,
        }
    }
}

/// Flatten a `ServiceError` into the same IPC-safe DTO every other error
/// type in this workspace becomes. `Db`/`Agent` delegate to their own
/// conversions; the two service-level variants report as `ErrorKind::Conflict`
/// (a second call may succeed once the first finishes) and
/// `ErrorKind::InvalidRequest` (calling `approve_tools` when nothing is
/// pending is a caller mistake, not a transient condition) respectively.
impl From<&ServiceError> for shared::error::ErrorReport {
    fn from(err: &ServiceError) -> Self {
        use shared::error::{ErrorKind, ErrorReport};
        match err {
            ServiceError::Db(inner) => inner.into(),
            ServiceError::Agent(inner) => inner.into(),
            ServiceError::ConversationBusy { .. } => ErrorReport {
                kind: ErrorKind::Conflict,
                provider: None,
                message: err.to_string(),
                retryable: true,
            },
            ServiceError::NoPendingTurn { .. } => ErrorReport {
                kind: ErrorKind::InvalidRequest,
                provider: None,
                message: err.to_string(),
                retryable: false,
            },
            ServiceError::TruncatedStream => ErrorReport {
                kind: ErrorKind::Agent,
                provider: None,
                message: err.to_string(),
                retryable: false,
            },
            ServiceError::ThemeNameConflict { .. } => ErrorReport {
                kind: ErrorKind::Conflict,
                provider: None,
                message: err.to_string(),
                retryable: false,
            },
            ServiceError::InvalidThemeColor { .. } | ServiceError::EmptyThemeName => ErrorReport {
                kind: ErrorKind::InvalidRequest,
                provider: None,
                message: err.to_string(),
                retryable: false,
            },
            ServiceError::ProjectNameConflict { .. } => ErrorReport {
                kind: ErrorKind::Conflict,
                provider: None,
                message: err.to_string(),
                retryable: false,
            },
            ServiceError::EmptyProjectName | ServiceError::InvalidProjectDir { .. } => ErrorReport {
                kind: ErrorKind::InvalidRequest,
                provider: None,
                message: err.to_string(),
                retryable: false,
            },
            ServiceError::Vfs(_) => ErrorReport {
                kind: ErrorKind::InvalidRequest,
                provider: None,
                message: err.to_string(),
                retryable: false,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::error::ErrorKind;

    #[test]
    fn conversation_busy_reports_as_conflict_and_retryable() {
        let err = ServiceError::ConversationBusy { conversation_id: 1 };
        let report: shared::error::ErrorReport = (&err).into();
        assert_eq!(report.kind, ErrorKind::Conflict);
        assert!(report.retryable);
    }

    #[test]
    fn no_pending_turn_reports_as_invalid_request_and_not_retryable() {
        let err = ServiceError::NoPendingTurn { conversation_id: 1 };
        let report: shared::error::ErrorReport = (&err).into();
        assert_eq!(report.kind, ErrorKind::InvalidRequest);
        assert!(!report.retryable);
    }

    #[test]
    fn a_wrapped_db_not_found_delegates_its_report() {
        let err: ServiceError = DbError::NotFound {
            entity: "conversation",
            id: "1".to_string(),
        }
        .into();
        let report: shared::error::ErrorReport = (&err).into();
        assert_eq!(report.kind, ErrorKind::NotFound);
    }

    #[test]
    fn a_wrapped_agent_error_delegates_its_report() {
        let err: ServiceError = AgentError::NotAwaitingApproval.into();
        let report: shared::error::ErrorReport = (&err).into();
        assert_eq!(report.kind, ErrorKind::Agent);
    }
}
