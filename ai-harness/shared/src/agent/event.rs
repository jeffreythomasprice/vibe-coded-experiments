//! Streaming agent events, emitted by `lib::agent::Agent::stream_turn` /
//! `resume_stream`.
//!
//! Shaped, like `shared::llm::StreamEvent`, so a future Tauri command can
//! forward these verbatim over an IPC channel with no intermediate type.

use serde::{Deserialize, Serialize};

use crate::llm::message::ContentBlock;
use crate::llm::stream::StreamEvent;

use super::turn::{Decision, DecidedBy};
use super::turn::AgentTurn;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    /// A new model call is starting. `step` is 0-based.
    StepStart { step: u32 },
    /// Passed through unmodified from `ChatProvider::stream`, tagged with the
    /// step it belongs to. **Do not fold events from more than one step with
    /// a single `lib::llm::accumulate::MessageAccumulator`** — block indices
    /// and usage are scoped to one provider response, so a second step's
    /// `MessageStart`/`MessageStop` doesn't sum onto the first, it overwrites
    /// it. Reset the accumulator on every `StepStart`, or just use the
    /// terminal [`AgentEvent::Turn`] event, which already carries the
    /// correctly-summed result.
    Model { step: u32, event: StreamEvent },
    /// A gated call was just resolved without suspending the turn — either by
    /// an automatic policy, or by a decision applied on resume. Always
    /// emitted immediately before the `ToolStart`/`ToolEnd` pair it explains,
    /// since that's the only thing a live view can key it to.
    ToolDecided {
        tool_use_id: String,
        name: String,
        decision: Decision,
        decided_by: DecidedBy,
    },
    ToolStart {
        tool_use_id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolEnd {
        tool_use_id: String,
        name: String,
        result: ContentBlock,
    },
    /// Terminal event. Carries exactly the `AgentTurn` the blocking API would
    /// have returned for the same conversation and script of provider
    /// responses.
    Turn(AgentTurn),
}

#[cfg(test)]
mod tests {
    use super::super::turn::TurnStop;
    use super::*;
    use crate::llm::message::{Conversation, StopReason, Usage};

    #[test]
    fn step_start_round_trips() {
        let json = serde_json::to_value(AgentEvent::StepStart { step: 2 }).unwrap();
        assert_eq!(json, serde_json::json!({"type": "step_start", "step": 2}));
    }

    #[test]
    fn model_event_carries_its_step_alongside_the_stream_event() {
        let event = AgentEvent::Model {
            step: 1,
            event: StreamEvent::MessageStart {
                model: "claude-opus-5".to_string(),
            },
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["step"], serde_json::json!(1));
        assert_eq!(json["event"]["type"], serde_json::json!("message_start"));
        let back: AgentEvent = serde_json::from_value(json).unwrap();
        assert!(matches!(back, AgentEvent::Model { step: 1, .. }));
    }

    #[test]
    fn tool_start_and_tool_end_round_trip() {
        let start = AgentEvent::ToolStart {
            tool_use_id: "call_0".to_string(),
            name: "get_weather".to_string(),
            input: serde_json::json!({"city": "Paris"}),
        };
        let json = serde_json::to_string(&start).unwrap();
        let back: AgentEvent = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, AgentEvent::ToolStart { .. }));

        let end = AgentEvent::ToolEnd {
            tool_use_id: "call_0".to_string(),
            name: "get_weather".to_string(),
            result: ContentBlock::ToolResult {
                tool_use_id: "call_0".to_string(),
                content: vec![],
                is_error: false,
            },
        };
        let json = serde_json::to_string(&end).unwrap();
        let back: AgentEvent = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, AgentEvent::ToolEnd { .. }));
    }

    #[test]
    fn tool_decided_round_trips_for_both_sources() {
        let by_user = AgentEvent::ToolDecided {
            tool_use_id: "call_1_0".to_string(),
            name: "deploy".to_string(),
            decision: Decision::Approve,
            decided_by: DecidedBy::User,
        };
        let json = serde_json::to_value(&by_user).unwrap();
        assert_eq!(json["type"], serde_json::json!("tool_decided"));
        assert_eq!(json["decided_by"]["type"], serde_json::json!("user"));
        let back: AgentEvent = serde_json::from_value(json).unwrap();
        assert!(matches!(back, AgentEvent::ToolDecided { decided_by: DecidedBy::User, .. }));

        let by_policy = AgentEvent::ToolDecided {
            tool_use_id: "call_1_1".to_string(),
            name: "bash".to_string(),
            decision: Decision::Deny { reason: Some("blocked".to_string()) },
            decided_by: DecidedBy::Policy { reason: "matched deny rule".to_string() },
        };
        let json = serde_json::to_string(&by_policy).unwrap();
        let back: AgentEvent = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, AgentEvent::ToolDecided { decided_by: DecidedBy::Policy { .. }, .. }));
    }

    #[test]
    fn turn_event_round_trips_the_whole_agent_turn() {
        let turn = AgentTurn {
            conversation: Conversation::default(),
            added: vec![],
            usage: Usage::default(),
            steps: 1,
            stop: TurnStop::Done {
                stop_reason: StopReason::EndTurn,
            },
            decisions: vec![],
        };
        let event = AgentEvent::Turn(turn);
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], serde_json::json!("turn"));
        let back: AgentEvent = serde_json::from_value(json).unwrap();
        match back {
            AgentEvent::Turn(turn) => assert_eq!(turn.steps, 1),
            other => panic!("expected Turn, got {other:?}"),
        }
    }
}
