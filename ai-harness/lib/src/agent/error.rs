//! Errors an [`crate::agent::Agent`] can produce.
//!
//! `thiserror`, following `lib::llm::error`'s precedent rather than this
//! workspace's usual `anyhow`-everywhere style — see that module's doc for
//! why: callers need to tell "the provider 429'd" from "you sent a decision
//! for a tool call that wasn't pending" apart, which `anyhow::Error` can't
//! express.

use thiserror::Error;

use shared::llm::StopReason;

use crate::llm::error::LlmError;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error(transparent)]
    Llm(#[from] LlmError),

    /// Two tools registered on the same `AgentBuilder` share a name.
    /// `AgentBuilder::build` rejects this outright rather than letting the
    /// second silently shadow the first.
    #[error("duplicate tool name {name:?}")]
    DuplicateTool { name: String },

    /// `Agent::resume` was called on an `AgentTurn` whose `stop` isn't
    /// `TurnStop::AwaitingApproval`.
    #[error("this turn is not awaiting approval")]
    NotAwaitingApproval,

    /// A tool call in `pending` had no matching entry in the decisions given
    /// to `Agent::resume`.
    #[error("no decision was given for pending tool call {tool_use_id:?}")]
    MissingDecision { tool_use_id: String },

    /// A decision named a `tool_use_id` that wasn't in `pending`.
    #[error("a decision was given for {tool_use_id:?}, which is not a pending tool call")]
    UnknownDecision { tool_use_id: String },

    /// The model's `stop_reason` was `ToolUse`, but no `ContentBlock::ToolUse`
    /// survived in its message — most likely a provider-native tool call
    /// (e.g. Anthropic's server-side web search) that deserialized to
    /// `ContentBlock::Unknown` because this crate doesn't model it. Raised
    /// instead of a false `TurnStop::Done`, and instead of looping — an
    /// assistant message made entirely of blocks this crate can't represent
    /// on the wire would 400 on replay (Anthropic drops both `Unknown` and
    /// top-level `Image` blocks when re-sending).
    #[error(
        "stop_reason was ToolUse but no ContentBlock::ToolUse was found (stop_reason: {stop_reason:?}) \
         — likely a provider-native tool call this crate does not model"
    )]
    UnhandledToolUse { stop_reason: StopReason },

    /// `Agent::resume`'s `turn.conversation` no longer matches the tool calls
    /// recorded in `pending`/`completed` — it was mutated after
    /// `TurnStop::AwaitingApproval` was returned. Sending it anyway would
    /// build a request with an unresolved (or duplicated) `tool_use`, which
    /// providers either reject outright or silently mishandle.
    #[error(
        "the conversation no longer matches the tool calls awaiting approval; \
         it must not be modified before calling resume"
    )]
    ConversationMismatch,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llm_error_converts_via_from() {
        let err: AgentError = LlmError::MissingApiKey {
            var: "ANTHROPIC_API_KEY".to_string(),
        }
        .into();
        assert!(matches!(err, AgentError::Llm(_)));
        assert!(err.to_string().contains("ANTHROPIC_API_KEY"));
    }

    #[test]
    fn duplicate_tool_names_the_tool() {
        let err = AgentError::DuplicateTool {
            name: "get_weather".to_string(),
        };
        assert!(err.to_string().contains("get_weather"));
    }

    #[test]
    fn missing_and_unknown_decision_name_the_tool_use_id() {
        let missing = AgentError::MissingDecision {
            tool_use_id: "call_0".to_string(),
        };
        assert!(missing.to_string().contains("call_0"));

        let unknown = AgentError::UnknownDecision {
            tool_use_id: "call_9".to_string(),
        };
        assert!(unknown.to_string().contains("call_9"));
    }

    #[test]
    fn unhandled_tool_use_names_the_stop_reason() {
        let err = AgentError::UnhandledToolUse {
            stop_reason: StopReason::ToolUse,
        };
        assert!(err.to_string().contains("ToolUse"));
    }
}
