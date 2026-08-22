//! Errors [`crate::service::Service`] can produce.

use thiserror::Error;

use crate::agent::AgentError;
use crate::db::DbError;

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
}

impl ServiceError {
    pub fn is_retryable(&self) -> bool {
        match self {
            ServiceError::Db(err) => err.is_retryable(),
            ServiceError::Agent(err) => err.is_retryable(),
            ServiceError::ConversationBusy { .. } => true,
            ServiceError::NoPendingTurn { .. } | ServiceError::TruncatedStream => false,
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
