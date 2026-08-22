//! Streaming event types.
//!
//! Named and shaped so a future Tauri command can forward these verbatim
//! over an IPC channel to the frontend without an intermediate type — see
//! `lib::llm::ChatProvider::stream`. Every provider's own SSE/NDJSON frames
//! are translated into this shape by that provider's `wire` module.

use serde::{Deserialize, Serialize};

use crate::llm::message::{ContentBlock, StopReason, Usage};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    MessageStart {
        model: String,
    },
    /// A new content block has started at `index`. `block` is an empty shell
    /// of the eventual block (e.g. `Text { text: String::new() }`); deltas
    /// for the same index fill it in.
    BlockStart {
        index: usize,
        block: ContentBlock,
    },
    Delta {
        index: usize,
        delta: Delta,
    },
    BlockStop {
        index: usize,
    },
    MessageStop {
        stop_reason: StopReason,
        usage: Usage,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Delta {
    Text {
        text: String,
    },
    Thinking {
        thinking: String,
    },
    /// The trailing signature for a `thinking` block, delivered as its own
    /// delta after the thinking text finishes. Must be applied verbatim —
    /// see `ContentBlock::Thinking`'s round-trip requirement.
    Signature {
        signature: String,
    },
    /// A fragment of a tool call's JSON input. Concatenate every delta for
    /// the same block index and parse once at `BlockStop`.
    ToolInputJson {
        partial_json: String,
    },
    PartialImage {
        index: u32,
        data: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_events_round_trip_through_json() {
        let events = vec![
            StreamEvent::MessageStart {
                model: "claude-opus-5".to_string(),
            },
            StreamEvent::BlockStart {
                index: 0,
                block: ContentBlock::Text {
                    text: String::new(),
                },
            },
            StreamEvent::Delta {
                index: 0,
                delta: Delta::Text {
                    text: "hi".to_string(),
                },
            },
            StreamEvent::BlockStop { index: 0 },
            StreamEvent::MessageStop {
                stop_reason: StopReason::EndTurn,
                usage: Usage::default(),
            },
        ];
        for event in events {
            let json = serde_json::to_string(&event).unwrap();
            let back: StreamEvent = serde_json::from_str(&json).unwrap();
            assert_eq!(
                serde_json::to_value(&back).unwrap(),
                serde_json::to_value(&event).unwrap(),
                "round trip changed shape for {json}"
            );
        }
    }

    #[test]
    fn tool_input_json_deltas_concatenate_to_valid_json() {
        let fragments = ["{\"loc", "ation\": \"Pa", "ris\"}"];
        let joined: String = fragments.iter().copied().collect();
        let parsed: serde_json::Value = serde_json::from_str(&joined).unwrap();
        assert_eq!(parsed, serde_json::json!({"location": "Paris"}));
    }
}
