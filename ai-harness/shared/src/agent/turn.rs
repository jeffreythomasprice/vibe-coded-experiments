//! The result of asking an agent for its next turn: `AgentTurn`, and how it
//! can end (`TurnStop`).

use serde::{Deserialize, Serialize};

use crate::llm::message::{ContentBlock, Conversation, Message, StopReason, Usage};

/// The result of one call to `lib::agent::Agent::next_turn` (or `send`,
/// `resume`, or the terminal event of the streaming equivalents).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTurn {
    /// The caller's new history: the conversation given to the agent, with
    /// this turn's messages appended. Hand it straight back on the next call.
    pub conversation: Conversation,
    /// Only what this turn added, for a caller that wants to render just the
    /// delta rather than replay the whole `conversation`.
    pub added: Vec<Message>,
    /// Summed across every model call this turn made.
    pub usage: Usage,
    /// How many model calls this turn made.
    pub steps: u32,
    pub stop: TurnStop,
    /// Every gated tool call this turn resolved, whether by the user or by
    /// an [`crate::agent::spec::Approval`]-consulting policy — appended to as
    /// the turn runs, never rewritten. `#[serde(default)]` so a `turn_json`
    /// row stored before this field existed still deserializes.
    #[serde(default)]
    pub decisions: Vec<ToolDecisionRecord>,
}

/// How a turn ended.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TurnStop {
    /// The model produced a final answer with no further tool calls pending.
    Done { stop_reason: StopReason },
    /// The model asked for at least one tool call that needs the user's
    /// approval before it can run. Call `lib::agent::Agent::resume` with a
    /// [`ToolDecision`] for every entry in `pending` to continue.
    AwaitingApproval {
        /// Tool calls needing a decision, in the order the model asked for
        /// them.
        pending: Vec<ToolApprovalRequest>,
        /// `ToolResult` blocks for calls from the *same* step that ran
        /// without approval. Held here rather than appended to
        /// `conversation`, because a provider requires a `tool_result` for
        /// **every** `tool_use` in the preceding assistant message — a
        /// partial set is not a sendable request on its own.
        completed: Vec<ContentBlock>,
    },
    /// The turn hit its step cap with the model still asking for tools.
    /// Deliberately a stop, not an error — the caller keeps the partial
    /// conversation and usage accrued so far.
    MaxSteps,
}

/// One tool call the model wants to make that needs a decision before it can
/// run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolApprovalRequest {
    pub tool_use_id: String,
    pub name: String,
    pub input: serde_json::Value,
}

/// The user's decision on one pending tool call, to hand back to
/// `lib::agent::Agent::resume`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDecision {
    pub tool_use_id: String,
    pub decision: Decision,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Decision {
    Approve,
    /// `reason` is what the model is told happened; a default placeholder
    /// message is used when `None`.
    Deny {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
}

/// Who resolved a gated tool call — a person, through the approval UI, or an
/// automatic [`crate::agent::spec::Approval`]-consulting policy (not yet
/// implemented by any rule; this is the hook for one). Distinct from
/// [`Decision`], which only says what was decided.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DecidedBy {
    User,
    Policy {
        /// Why the policy ruled the way it did — shown to the user
        /// alongside the decision, e.g. "matched auto-approve rule for
        /// read-only paths".
        reason: String,
    },
}

/// A permanent record of one gated call's resolution — appended to
/// [`AgentTurn::decisions`] as it happens, and eventually mirrored into
/// `tool_calls.decided_by`/`decision_reason`/`decided_at` by
/// `lib::db::turns::settle`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDecisionRecord {
    pub tool_use_id: String,
    pub decision: Decision,
    pub decided_by: DecidedBy,
    /// ISO8601 UTC, millisecond precision.
    pub decided_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::message::Role;

    fn sample_conversation() -> Conversation {
        Conversation {
            system: Some("be helpful".to_string()),
            messages: vec![Message::user(vec![ContentBlock::Text {
                text: "hi".to_string(),
            }])],
        }
    }

    #[test]
    fn agent_turn_done_round_trips_through_json() {
        let turn = AgentTurn {
            conversation: sample_conversation(),
            added: vec![Message::assistant(vec![ContentBlock::Text {
                text: "hello!".to_string(),
            }])],
            usage: Usage {
                input_tokens: 10,
                output_tokens: 5,
                ..Default::default()
            },
            steps: 1,
            stop: TurnStop::Done {
                stop_reason: StopReason::EndTurn,
            },
            decisions: vec![],
        };
        let json = serde_json::to_string(&turn).unwrap();
        let back: AgentTurn = serde_json::from_str(&json).unwrap();
        assert_eq!(back.steps, 1);
        assert_eq!(back.usage.input_tokens, 10);
        assert!(matches!(back.stop, TurnStop::Done { .. }));
    }

    /// A `turns.turn_json` row written before `AgentTurn::decisions` existed
    /// has no `"decisions"` key at all. `#[serde(default)]` is what makes it
    /// still deserialize after this field ships — without it, every
    /// conversation with a turn suspended across the upgrade would fail to
    /// load.
    #[test]
    fn agent_turn_without_a_decisions_field_deserializes_with_it_defaulted_empty() {
        let json = serde_json::json!({
            "conversation": {"system": null, "messages": []},
            "added": [],
            "usage": {"input_tokens": 0, "output_tokens": 0},
            "steps": 1,
            "stop": {"type": "done", "stop_reason": "end_turn"},
        });
        let turn: AgentTurn = serde_json::from_value(json).unwrap();
        assert!(turn.decisions.is_empty());
    }

    #[test]
    fn tool_decision_record_round_trips_with_a_policy_reason() {
        let record = ToolDecisionRecord {
            tool_use_id: "call_1_0".to_string(),
            decision: Decision::Deny { reason: Some("too risky".to_string()) },
            decided_by: DecidedBy::Policy { reason: "matched deny rule".to_string() },
            decided_at: "2026-08-29T00:00:00.000Z".to_string(),
        };
        let json = serde_json::to_value(&record).unwrap();
        assert_eq!(json["decided_by"]["type"], serde_json::json!("policy"));
        assert_eq!(json["decided_by"]["reason"], serde_json::json!("matched deny rule"));
        let back: ToolDecisionRecord = serde_json::from_value(json).unwrap();
        assert_eq!(back, record);
    }

    #[test]
    fn decided_by_user_round_trips_with_no_extra_fields() {
        let json = serde_json::to_value(DecidedBy::User).unwrap();
        assert_eq!(json, serde_json::json!({"type": "user"}));
        let back: DecidedBy = serde_json::from_value(json).unwrap();
        assert_eq!(back, DecidedBy::User);
    }

    #[test]
    fn turn_stop_awaiting_approval_round_trips_with_pending_and_completed() {
        let stop = TurnStop::AwaitingApproval {
            pending: vec![ToolApprovalRequest {
                tool_use_id: "call_0".to_string(),
                name: "deploy".to_string(),
                input: serde_json::json!({"env": "prod"}),
            }],
            completed: vec![ContentBlock::ToolResult {
                tool_use_id: "call_1".to_string(),
                content: vec![],
                is_error: false,
            }],
        };
        let json = serde_json::to_value(&stop).unwrap();
        assert_eq!(json["type"], serde_json::json!("awaiting_approval"));
        let back: TurnStop = serde_json::from_value(json).unwrap();
        match back {
            TurnStop::AwaitingApproval { pending, completed } => {
                assert_eq!(pending.len(), 1);
                assert_eq!(pending[0].tool_use_id, "call_0");
                assert_eq!(completed.len(), 1);
            }
            other => panic!("expected AwaitingApproval, got {other:?}"),
        }
    }

    #[test]
    fn turn_stop_max_steps_round_trips() {
        let json = serde_json::to_value(TurnStop::MaxSteps).unwrap();
        assert_eq!(json, serde_json::json!({"type": "max_steps"}));
        let back: TurnStop = serde_json::from_value(json).unwrap();
        assert!(matches!(back, TurnStop::MaxSteps));
    }

    #[test]
    fn decision_deny_defaults_to_no_reason() {
        let json = serde_json::to_value(Decision::Deny { reason: None }).unwrap();
        assert_eq!(json, serde_json::json!({"type": "deny"}));
        let back: Decision = serde_json::from_value(json).unwrap();
        assert!(matches!(back, Decision::Deny { reason: None }));
    }

    #[test]
    fn decision_deny_carries_a_reason_when_given() {
        let decision = Decision::Deny {
            reason: Some("too risky".to_string()),
        };
        let json = serde_json::to_value(&decision).unwrap();
        assert_eq!(json["reason"], serde_json::json!("too risky"));
    }

    #[test]
    fn tool_decision_round_trips() {
        let decision = ToolDecision {
            tool_use_id: "call_0".to_string(),
            decision: Decision::Approve,
        };
        let json = serde_json::to_string(&decision).unwrap();
        let back: ToolDecision = serde_json::from_str(&json).unwrap();
        assert_eq!(back.tool_use_id, "call_0");
        assert!(matches!(back.decision, Decision::Approve));
    }

    #[test]
    fn added_messages_are_assistant_role_by_construction_in_this_test() {
        // Sanity check that the fixture helper used across this module's
        // tests builds what it claims to.
        let turn = AgentTurn {
            conversation: sample_conversation(),
            added: vec![Message::assistant(vec![])],
            usage: Usage::default(),
            steps: 1,
            stop: TurnStop::Done {
                stop_reason: StopReason::EndTurn,
            },
            decisions: vec![],
        };
        assert_eq!(turn.added[0].role, Role::Assistant);
    }
}
