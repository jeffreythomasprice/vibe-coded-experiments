//! The hook for an automatic approval rule — consulted by [`super::Agent`]'s
//! loop for every tool call reporting [`shared::agent::Approval::RequiresApproval`],
//! before it would otherwise suspend the turn for the user.
//!
//! No rule ships here yet; [`AskUser`] — every gated call goes to the person
//! — is the only implementation and the default every [`super::Agent`] starts
//! with. A future rule set (an allow-list of paths, a denylist of shell
//! patterns, …) implements [`ApprovalPolicy`] and is injected via
//! `AgentBuilder::approval_policy`.

use shared::agent::ToolApprovalRequest;

/// What an [`ApprovalPolicy`] decided about one gated call.
#[derive(Debug, Clone, PartialEq)]
pub enum PolicyOutcome {
    /// Run the call without asking — `reason` is recorded alongside the
    /// decision so a person reviewing history later can see why.
    Approve { reason: String },
    /// Refuse the call without asking — `reason` is both recorded and told
    /// to the model, so it can react (try something else, explain to the
    /// user) rather than just seeing an opaque failure.
    Deny { reason: String },
    /// Defer to the user — the ordinary path today, since no automatic rule
    /// exists yet.
    AskUser,
}

/// A pluggable rule for deciding a gated tool call without the user. A
/// trait, matching every other pluggable seam in this crate (`Tool`,
/// `SandboxBackend`, `EventSink`) rather than a bare closure type, since a
/// real rule set will likely carry state (a path allow-list, a cache) that a
/// closure makes awkward to hold.
pub trait ApprovalPolicy: Send + Sync {
    fn evaluate(&self, request: &ToolApprovalRequest) -> PolicyOutcome;
}

/// The default policy: defer every gated call to the user. What every
/// [`super::Agent`] starts with until a caller injects something else via
/// `AgentBuilder::approval_policy`.
pub struct AskUser;

impl ApprovalPolicy for AskUser {
    fn evaluate(&self, _request: &ToolApprovalRequest) -> PolicyOutcome {
        PolicyOutcome::AskUser
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> ToolApprovalRequest {
        ToolApprovalRequest {
            tool_use_id: "call_1_0".to_string(),
            name: "deploy".to_string(),
            input: serde_json::json!({"env": "prod"}),
        }
    }

    #[test]
    fn ask_user_always_defers() {
        assert_eq!(AskUser.evaluate(&request()), PolicyOutcome::AskUser);
    }
}
