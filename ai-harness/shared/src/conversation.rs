//! Read/write DTOs for a persisted conversation — what `lib::service`'s
//! conversation methods return over the Tauri IPC boundary.
//!
//! Distinct from `llm::Conversation`, which is the bare `{system, messages}`
//! shape a `lib::agent::Agent` call takes and returns: these types carry the
//! database identity, per-turn bookkeeping, and the suspended-approval tail
//! that `llm::Conversation` has no room for.

use serde::{Deserialize, Serialize};

use crate::agent::config::AgentConfig;
use crate::agent::turn::{DecidedBy, Decision, ToolApprovalRequest};
use crate::error::ErrorReport;
use crate::ids::{ConversationId, MessageId, TurnId};
use crate::llm::message::{ContentBlock, Role, StopReason, Usage};

/// One row of `messages`, as read back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredMessage {
    pub id: MessageId,
    /// 0-based position in the conversation — the authoritative order.
    pub seq: u32,
    /// The turn that produced this message. `None` only for a user message
    /// written before its turn was ever created — never happens today, since
    /// `lib::service` writes the user message and the turn row together, but
    /// left optional rather than assumed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<TurnId>,
    pub role: Role,
    pub content: Vec<ContentBlock>,
    /// ISO8601 UTC, millisecond precision.
    pub created_at: String,
}

/// One row of `conversations`, for a sidebar-style list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConversationSummary {
    pub id: ConversationId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The agent this conversation is currently linked to, if that agent
    /// config still exists. `None` after the agent config it started from
    /// was deleted — the conversation stays runnable regardless, since its
    /// own frozen copy of the config is what actually gets used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_config_id: Option<crate::ids::AgentConfigId>,
    pub agent_name: String,
    /// The project this conversation runs under, if any — unlike
    /// `agent_config_id`/`agent_name`, resolved *live* every time this
    /// summary is built, not frozen at creation (see
    /// `lib::db::projects`'s module doc for why). `None` covers both "the
    /// default project" and "its project was since deleted" — both mean no
    /// filesystem access, so the ambiguity is harmless.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<crate::ids::ProjectId>,
    /// The project's current name, alongside `project_id` for display —
    /// `None` exactly when `project_id` is `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_message_at: Option<String>,
    /// Cheap sidebar badge: does this conversation have a turn suspended on
    /// `TurnStop::AwaitingApproval`?
    pub awaiting_approval: bool,
}

/// The tail of a turn suspended for approval. Its messages are deliberately
/// absent from `ConversationView::messages` — see `lib::db`'s module doc —
/// so a renderer draws `added` after `messages`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingApproval {
    pub turn_id: TurnId,
    pub added: Vec<crate::llm::message::Message>,
    pub requests: Vec<ToolApprovalRequest>,
    /// Calls from the same step that already ran without needing approval —
    /// shown so the user can see what happened before deciding on the rest.
    pub completed: Vec<ContentBlock>,
    pub usage: Usage,
    pub steps: u32,
}

/// A permanent record of one gated call's resolution, read back from
/// `tool_calls` for display — the historical counterpart of
/// [`crate::agent::turn::ToolDecisionRecord`], which is how it got there.
///
/// Keyed by `(turn_id, tool_use_id)`, never `tool_use_id` alone:
/// `lib::agent::rewrite_tool_use_ids` restarts its counter every turn, so the
/// same id recurs across a conversation's turns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDecisionView {
    pub turn_id: TurnId,
    pub tool_use_id: String,
    pub decision: Decision,
    /// `None` only in the brief window between the user answering (which
    /// records `decision` immediately) and the turn actually settling
    /// (which records who/why) — see `lib::db::turns::write_decisions`'s doc.
    /// Self-heals on the next settle; never means "nobody decided".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decided_by: Option<DecidedBy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decided_at: Option<String>,
}

/// A full conversation, assembled for display.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConversationView {
    pub summary: ConversationSummary,
    /// The `AgentConfig` frozen as of when this conversation started — see
    /// `lib::db`'s module doc on `conversations.agent_config_json` for why
    /// this is a snapshot rather than a live join.
    pub agent: AgentConfig,
    pub messages: Vec<StoredMessage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending: Option<PendingApproval>,
    /// Every gated call this conversation has ever resolved, across every
    /// turn — what lets a returning user see whether a past tool bubble was
    /// approved, denied, and by whom.
    #[serde(default)]
    pub decisions: Vec<ToolDecisionView>,
}

/// What a turn ended as, returned by `send_message`/`approve_tools` once
/// their stream reaches a terminal event.
///
/// Deliberately not the whole `AgentTurn`: the caller already received every
/// message as it streamed over the event channel, and a suspended
/// `AgentTurn` is a continuation token that belongs in the database, not
/// something a frontend hands back byte-exactly on `approve_tools`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TurnOutcome {
    Done {
        turn_id: TurnId,
        steps: u32,
        usage: Usage,
        stop_reason: StopReason,
        messages: Vec<StoredMessage>,
    },
    AwaitingApproval {
        turn_id: TurnId,
        pending: Vec<ToolApprovalRequest>,
        completed: Vec<ContentBlock>,
    },
    MaxSteps {
        turn_id: TurnId,
        steps: u32,
        usage: Usage,
        messages: Vec<StoredMessage>,
    },
}

/// What `attach_conversation` reports once it stops following a
/// conversation's in-flight run.
///
/// Struct variants, not a newtype wrapping `TurnOutcome`: `TurnOutcome` is
/// itself `#[serde(tag = "type")]`, so a newtype variant would nest a second
/// competing `"type"` key inside this one's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RunStatus {
    /// Nothing was in flight for this conversation — the persisted view
    /// (from `get_conversation`) is already current.
    Idle,
    Finished { outcome: TurnOutcome },
    Failed { error: ErrorReport },
}

/// Query parameters for listing conversations.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ListConversations {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::config::AgentConfigInput;
    use crate::agent::spec::Approval;
    use crate::agent::turn::ToolApprovalRequest;
    use crate::ids::AgentConfigId;
    use crate::llm::message::Message;
    use crate::llm::model::ModelRef;
    use crate::llm::tool::Thinking;

    fn agent_config() -> AgentConfig {
        AgentConfig {
            id: AgentConfigId(1),
            input: AgentConfigInput {
                name: "assistant".to_string(),
                description: None,
                model: ModelRef::new("anthropic", "claude-opus-5"),
                system: vec![],
                max_tokens: 1024,
                tools: vec![],
                tool_choice: None,
                thinking: Thinking::default(),
                stop_sequences: vec![],
                max_steps: 8,
            },
            created_at: "2026-08-22T00:00:00.000Z".to_string(),
            updated_at: "2026-08-22T00:00:00.000Z".to_string(),
        }
    }

    #[test]
    fn conversation_summary_round_trips_and_omits_absent_agent() {
        let summary = ConversationSummary {
            id: ConversationId(1),
            title: None,
            agent_config_id: None,
            agent_name: "assistant".to_string(),
            project_id: None,
            project_name: None,
            created_at: "2026-08-22T00:00:00.000Z".to_string(),
            updated_at: "2026-08-22T00:00:00.000Z".to_string(),
            last_message_at: None,
            awaiting_approval: false,
        };
        let json = serde_json::to_value(&summary).unwrap();
        assert!(json.get("agent_config_id").is_none());
        assert!(json.get("title").is_none());
        assert!(json.get("last_message_at").is_none());
        assert!(json.get("project_id").is_none());
        assert!(json.get("project_name").is_none());
        let back: ConversationSummary = serde_json::from_value(json).unwrap();
        assert_eq!(back, summary);
    }

    #[test]
    fn conversation_view_round_trips_with_pending_approval() {
        let view = ConversationView {
            summary: ConversationSummary {
                id: ConversationId(1),
                title: Some("Deploy".to_string()),
                agent_config_id: Some(AgentConfigId(1)),
                agent_name: "assistant".to_string(),
                project_id: None,
                project_name: None,
                created_at: "2026-08-22T00:00:00.000Z".to_string(),
                updated_at: "2026-08-22T00:00:01.000Z".to_string(),
                last_message_at: Some("2026-08-22T00:00:01.000Z".to_string()),
                awaiting_approval: true,
            },
            agent: agent_config(),
            messages: vec![],
            pending: Some(PendingApproval {
                turn_id: TurnId(1),
                added: vec![Message::assistant(vec![])],
                requests: vec![ToolApprovalRequest {
                    tool_use_id: "call_1_0".to_string(),
                    name: "deploy".to_string(),
                    input: serde_json::json!({"env": "prod"}),
                }],
                completed: vec![],
                usage: Usage::default(),
                steps: 1,
            }),
            decisions: vec![ToolDecisionView {
                turn_id: TurnId(1),
                tool_use_id: "call_1_0".to_string(),
                decision: Decision::Approve,
                decided_by: Some(DecidedBy::User),
                decided_at: Some("2026-08-22T00:00:01.000Z".to_string()),
            }],
        };
        let json = serde_json::to_string(&view).unwrap();
        let back: ConversationView = serde_json::from_str(&json).unwrap();
        assert_eq!(back, view);
        assert!(back.pending.is_some());
        assert_eq!(back.decisions.len(), 1);
    }

    #[test]
    fn tool_decision_view_omits_decided_by_and_decided_at_when_absent() {
        let view = ToolDecisionView {
            turn_id: TurnId(1),
            tool_use_id: "call_1_0".to_string(),
            decision: Decision::Approve,
            decided_by: None,
            decided_at: None,
        };
        let json = serde_json::to_value(&view).unwrap();
        assert!(json.get("decided_by").is_none());
        assert!(json.get("decided_at").is_none());
        let back: ToolDecisionView = serde_json::from_value(json).unwrap();
        assert_eq!(back, view);
    }

    #[test]
    fn turn_outcome_variants_round_trip() {
        let done = TurnOutcome::Done {
            turn_id: TurnId(1),
            steps: 1,
            usage: Usage::default(),
            stop_reason: StopReason::EndTurn,
            messages: vec![],
        };
        let json = serde_json::to_value(&done).unwrap();
        assert_eq!(json["type"], serde_json::json!("done"));
        let back: TurnOutcome = serde_json::from_value(json).unwrap();
        assert_eq!(back, done);

        let awaiting = TurnOutcome::AwaitingApproval {
            turn_id: TurnId(2),
            pending: vec![ToolApprovalRequest {
                tool_use_id: "call_1_0".to_string(),
                name: "deploy".to_string(),
                input: serde_json::json!({}),
            }],
            completed: vec![],
        };
        let json = serde_json::to_value(&awaiting).unwrap();
        assert_eq!(json["type"], serde_json::json!("awaiting_approval"));

        let max_steps = TurnOutcome::MaxSteps {
            turn_id: TurnId(3),
            steps: 8,
            usage: Usage::default(),
            messages: vec![],
        };
        let json = serde_json::to_value(&max_steps).unwrap();
        assert_eq!(json["type"], serde_json::json!("max_steps"));
    }

    #[test]
    fn run_status_variants_round_trip_with_the_expected_wire_tags() {
        let idle = RunStatus::Idle;
        let json = serde_json::to_value(&idle).unwrap();
        assert_eq!(json, serde_json::json!({"type": "idle"}));
        let back: RunStatus = serde_json::from_value(json).unwrap();
        assert_eq!(back, idle);

        let finished = RunStatus::Finished {
            outcome: TurnOutcome::Done {
                turn_id: TurnId(1),
                steps: 1,
                usage: Usage::default(),
                stop_reason: StopReason::EndTurn,
                messages: vec![],
            },
        };
        let json = serde_json::to_value(&finished).unwrap();
        assert_eq!(json["type"], serde_json::json!("finished"));
        assert_eq!(json["outcome"]["type"], serde_json::json!("done"));
        let back: RunStatus = serde_json::from_value(json).unwrap();
        assert_eq!(back, finished);

        let failed = RunStatus::Failed {
            error: ErrorReport {
                kind: crate::error::ErrorKind::Server,
                provider: Some("scripted".to_string()),
                message: "boom".to_string(),
                retryable: true,
            },
        };
        let json = serde_json::to_value(&failed).unwrap();
        assert_eq!(json["type"], serde_json::json!("failed"));
        assert_eq!(json["error"]["kind"], serde_json::json!("server"));
        let back: RunStatus = serde_json::from_value(json).unwrap();
        assert_eq!(back, failed);
    }

    #[test]
    fn list_conversations_defaults_to_no_filters() {
        let json = serde_json::to_value(ListConversations::default()).unwrap();
        assert_eq!(json, serde_json::json!({}));
    }

    // Referenced so `Approval` stays imported even if the round-trip tests
    // above are trimmed later.
    #[test]
    fn approval_default_is_automatic() {
        assert_eq!(Approval::default(), Approval::Automatic);
    }
}
