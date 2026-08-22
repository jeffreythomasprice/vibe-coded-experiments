//! A serializable, coarse-grained error report for crossing the Tauri IPC
//! boundary.
//!
//! `lib::llm::LlmError` and `lib::agent::AgentError` are native-only
//! `thiserror` enums — precise enough to match on, but not `Serialize`, and
//! `shared` deliberately depends on neither `thiserror` nor `anyhow` (see
//! this crate's module doc for the wasm-frontend constraint that rules that
//! out). [`ErrorReport`] is the flattened DTO that carries just enough of
//! that classification across IPC for a UI to decide what to say and
//! whether to offer a retry button. The `From` conversions live in `lib`
//! (the only place that can see both the native error types and this one).

use serde::{Deserialize, Serialize};

/// What went wrong, coarsely enough for a UI to decide what to say and
/// whether to offer a retry button. Mirrors the cases `lib::llm::LlmError`
/// distinguishes, without the native-only detail (status codes, decode
/// errors, provider request ids).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    /// The provider refuses to bill the request: credits exhausted, a
    /// spend limit, a suspended account. Not fixable by retrying or
    /// resizing the request.
    InsufficientCredit,
    /// The request's input (a chat message or an embeddings batch) is too
    /// large for the model.
    InputTooLarge,
    /// A transient rate limit — retrying later, per `retryable`, may help.
    RateLimit,
    /// A missing, invalid, or unauthorized credential.
    Auth,
    /// The provider rejected the request's shape for a reason other than
    /// the more specific kinds above.
    InvalidRequest,
    NotFound,
    /// The provider (or an intermediate proxy) failed on its end.
    Server,
    /// A connection-level failure — DNS, TCP reset, TLS handshake, timeout.
    Network,
    /// A local misconfiguration (e.g. a missing API key) — never reached a
    /// provider at all.
    Config,
    /// An agent-loop invariant, not a provider failure (e.g. a decision was
    /// given for a tool call that was not pending).
    Agent,
    /// The app's own local database failed — not a provider. A UI should say
    /// "something's wrong with this install," not "the model is down."
    Storage,
    /// The requested operation collides with one already in flight (e.g. a
    /// second turn on a conversation that is already streaming). Distinct
    /// from `Storage`: the fix here is "wait" or "retry," not "file a bug."
    Conflict,
    Other,
}

/// A flattened, IPC-safe view of an `LlmError` or `AgentError`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorReport {
    pub kind: ErrorKind,
    /// The provider this error came from, when there is one — absent for
    /// agent-loop errors that never reached a provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// The error's `Display` text, for a UI that just wants to show
    /// something readable.
    pub message: String,
    /// Whether retrying the same request might succeed.
    pub retryable: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_report_round_trips_through_json() {
        let report = ErrorReport {
            kind: ErrorKind::InsufficientCredit,
            provider: Some("openai".to_string()),
            message: "openai: insufficient credit: you exceeded your current quota".to_string(),
            retryable: false,
        };
        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["kind"], serde_json::json!("insufficient_credit"));
        assert_eq!(json["provider"], serde_json::json!("openai"));
        assert_eq!(json["retryable"], serde_json::json!(false));
        let back: ErrorReport = serde_json::from_value(json).unwrap();
        assert_eq!(back, report);
    }

    #[test]
    fn provider_is_omitted_from_json_when_absent() {
        let report = ErrorReport {
            kind: ErrorKind::Agent,
            provider: None,
            message: "this turn is not awaiting approval".to_string(),
            retryable: false,
        };
        let json = serde_json::to_value(&report).unwrap();
        assert!(json.get("provider").is_none());
    }
}
