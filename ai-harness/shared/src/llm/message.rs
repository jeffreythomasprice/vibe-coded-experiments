//! Conversation and message types.
//!
//! Modeled on Anthropic's content-block shape, because it is the most
//! expressive of the three providers `lib::llm` supports — Ollama and
//! OpenAI's `/v1/responses` both convert onto it without losing information
//! on the way in. The one lossy direction is `lib::llm::ollama::wire`
//! flattening blocks back into Ollama's plain-string `content` field.

use serde::{Deserialize, Serialize};

use crate::llm::image::ImageSource;

/// A full conversation to send to a provider: an optional system prompt plus
/// the turn-by-turn history.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Conversation {
    /// Anthropic's top-level `system`, OpenAI's `instructions`, or Ollama's
    /// leading `{"role": "system"}` message — each provider's `wire` module
    /// places this appropriately.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    pub messages: Vec<Message>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Vec<ContentBlock>,
}

impl Message {
    pub fn user(content: Vec<ContentBlock>) -> Self {
        Self {
            role: Role::User,
            content,
        }
    }

    pub fn assistant(content: Vec<ContentBlock>) -> Self {
        Self {
            role: Role::Assistant,
            content,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

/// One block of a message's content.
///
/// Internally tagged on `type`, matching every provider's own wire shape.
/// Anything this crate doesn't recognize deserializes to
/// [`ContentBlock::Unknown`] rather than failing the whole message — but an
/// `Unknown` block carries no payload, so it must never be sent back to a
/// provider: `lib::llm`'s request builders drop it with a `tracing::warn!`
/// rather than round-tripping `{"type":"unknown"}`, which providers reject.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    /// Extended-thinking output. `signature` must round-trip byte-for-byte
    /// when a message is replayed back to the same provider — treat it as
    /// opaque, never re-derive or normalize it.
    Thinking {
        thinking: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
    },
    /// Thinking content the provider declined to return in the clear.
    /// Preserved opaquely and replayed as-is; never filter these out of
    /// conversation history.
    RedactedThinking {
        data: String,
    },
    Image {
        source: ImageSource,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: Vec<ToolResultContent>,
        #[serde(default)]
        is_error: bool,
    },
    /// Forward-compatibility catch-all for a block type this crate doesn't
    /// model yet.
    #[serde(other)]
    Unknown,
}

/// Content of a `tool_result` block. Anthropic allows a bare string here;
/// callers should normalize to `Text` on the way in so this crate only has
/// one shape to deal with.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolResultContent {
    Text { text: String },
    Image { source: ImageSource },
}

/// Why the model stopped generating.
///
/// Serializes as a bare string (serde's default representation for a
/// fieldless enum). `#[serde(other)]` only works on a unit variant, so this
/// enum's forward-compat fallback is a variant-level `#[serde(untagged)]`
/// newtype instead — it preserves whatever string a provider actually sent,
/// rather than discarding it. Requires serde >= 1.0.181 (workspace has
/// 1.0.229 locked).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    MaxTokens,
    ToolUse,
    StopSequence,
    Refusal,
    PauseTurn,
    #[serde(untagged)]
    Other(String),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: u32,
    #[serde(default)]
    pub output_tokens: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_input_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<u32>,
}

/// The result of a non-streaming `ChatProvider::complete` call, or of folding
/// a stream via `lib::llm::accumulate`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletedMessage {
    pub role: Role,
    pub content: Vec<ContentBlock>,
    pub stop_reason: StopReason,
    pub usage: Usage,
    /// The model that actually produced this response — useful when a
    /// request didn't pin one, or a provider substituted a fallback.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_blocks_round_trip_through_json() {
        let blocks = vec![
            ContentBlock::Text {
                text: "hello".to_string(),
            },
            ContentBlock::Thinking {
                thinking: "let me think".to_string(),
                signature: Some("sig123".to_string()),
            },
            ContentBlock::RedactedThinking {
                data: "opaque".to_string(),
            },
            ContentBlock::ToolUse {
                id: "toolu_1".to_string(),
                name: "get_weather".to_string(),
                input: serde_json::json!({"location": "Paris"}),
            },
            ContentBlock::ToolResult {
                tool_use_id: "toolu_1".to_string(),
                content: vec![ToolResultContent::Text {
                    text: "72F sunny".to_string(),
                }],
                is_error: false,
            },
        ];
        for block in blocks {
            let json = serde_json::to_string(&block).unwrap();
            let back: ContentBlock = serde_json::from_str(&json).unwrap();
            assert_eq!(
                serde_json::to_value(&back).unwrap(),
                serde_json::to_value(&block).unwrap(),
                "round trip changed shape for {json}"
            );
        }
    }

    #[test]
    fn unknown_content_block_types_deserialize_to_unknown() {
        let json = r#"{"type": "server_tool_use", "id": "x", "name": "web_search"}"#;
        let block: ContentBlock = serde_json::from_str(json).unwrap();
        assert!(matches!(block, ContentBlock::Unknown));
    }

    #[test]
    fn tool_use_input_survives_arbitrary_json() {
        let input = serde_json::json!({
            "nested": {"a": [1, 2, 3], "b": null},
            "unicode": "héllo wörld",
        });
        let block = ContentBlock::ToolUse {
            id: "toolu_2".to_string(),
            name: "search".to_string(),
            input: input.clone(),
        };
        let json = serde_json::to_string(&block).unwrap();
        let back: ContentBlock = serde_json::from_str(&json).unwrap();
        match back {
            ContentBlock::ToolUse { input: back_input, .. } => {
                assert_eq!(back_input, input);
            }
            other => panic!("expected ToolUse, got {other:?}"),
        }
    }

    #[test]
    fn stop_reason_round_trips_as_a_bare_string() {
        let json = serde_json::to_value(StopReason::EndTurn).unwrap();
        assert_eq!(json, serde_json::json!("end_turn"));
        let back: StopReason = serde_json::from_value(json).unwrap();
        assert_eq!(back, StopReason::EndTurn);
    }

    #[test]
    fn unknown_stop_reasons_deserialize_to_other() {
        let parsed: StopReason = serde_json::from_str(r#""model_context_window_exceeded""#).unwrap();
        assert_eq!(
            parsed,
            StopReason::Other("model_context_window_exceeded".to_string())
        );
        // And it serializes back out as the same bare string, not wrapped.
        assert_eq!(
            serde_json::to_value(&parsed).unwrap(),
            serde_json::json!("model_context_window_exceeded")
        );
    }

    #[test]
    fn tool_result_normalizes_to_content_blocks() {
        let json = r#"{
            "type": "tool_result",
            "tool_use_id": "toolu_3",
            "content": [{"type": "text", "text": "ok"}],
            "is_error": false
        }"#;
        let block: ContentBlock = serde_json::from_str(json).unwrap();
        match block {
            ContentBlock::ToolResult { content, is_error, .. } => {
                assert!(!is_error);
                assert_eq!(content.len(), 1);
            }
            other => panic!("expected ToolResult, got {other:?}"),
        }
    }
}
