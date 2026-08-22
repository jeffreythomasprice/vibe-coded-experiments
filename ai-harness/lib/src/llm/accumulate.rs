//! Fold a sequence of `shared::llm::StreamEvent`s into the same
//! `CompletedMessage` shape a blocking `ChatProvider::complete` call returns.
//!
//! Every provider's live test streams a prompt and asserts the accumulated
//! result matches the blocking response to the same prompt — the single most
//! valuable test per provider, because it proves the streaming parser and
//! the blocking parser agree on what a message looks like.

use std::collections::BTreeMap;

use shared::llm::{CompletedMessage, ContentBlock, Delta, Role, StopReason, StreamEvent, Usage};

use crate::llm::error::{ApiErrorKind, LlmError};

/// Accumulates `StreamEvent`s in order. Block index is the correlation key —
/// events for the same index are expected to arrive together (`BlockStart`,
/// then zero or more `Delta`s, then `BlockStop`), but indices themselves may
/// be interleaved or arrive out of numeric order; the final message is always
/// assembled in ascending index order.
#[derive(Debug, Default)]
pub struct MessageAccumulator {
    model: Option<String>,
    blocks: BTreeMap<usize, PartialBlock>,
    stop_reason: Option<StopReason>,
    usage: Usage,
}

#[derive(Debug)]
enum PartialBlock {
    Text(String),
    Thinking {
        thinking: String,
        signature: Option<String>,
    },
    ToolUse {
        id: String,
        name: String,
        /// Concatenated `input_json_delta` fragments; parsed once at `finish`.
        json: String,
    },
    /// A block type that (as far as this crate knows) never streams
    /// incrementally — it arrives whole via `BlockStart` and is passed
    /// through unchanged.
    Other(ContentBlock),
}

impl MessageAccumulator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one event. Errors if a delta's shape doesn't match the block it
    /// targets (e.g. a `Delta::Text` for a block that started as `ToolUse`).
    pub fn push(&mut self, event: StreamEvent) -> Result<(), LlmError> {
        match event {
            StreamEvent::MessageStart { model } => {
                self.model = Some(model);
            }
            StreamEvent::BlockStart { index, block } => {
                let partial = match block {
                    ContentBlock::Text { text } => PartialBlock::Text(text),
                    ContentBlock::Thinking { thinking, signature } => PartialBlock::Thinking {
                        thinking,
                        signature,
                    },
                    ContentBlock::ToolUse { id, name, input } => PartialBlock::ToolUse {
                        id,
                        name,
                        json: if input.is_null() {
                            String::new()
                        } else {
                            input.to_string()
                        },
                    },
                    other => PartialBlock::Other(other),
                };
                self.blocks.insert(index, partial);
            }
            StreamEvent::Delta { index, delta } => {
                let block = self
                    .blocks
                    .entry(index)
                    .or_insert_with(|| PartialBlock::Text(String::new()));
                match (block, delta) {
                    (PartialBlock::Text(text), Delta::Text { text: fragment }) => {
                        text.push_str(&fragment);
                    }
                    (
                        PartialBlock::Thinking { thinking, .. },
                        Delta::Thinking { thinking: fragment },
                    ) => {
                        thinking.push_str(&fragment);
                    }
                    (
                        PartialBlock::Thinking { signature, .. },
                        Delta::Signature { signature: value },
                    ) => {
                        *signature = Some(value);
                    }
                    (PartialBlock::ToolUse { json, .. }, Delta::ToolInputJson { partial_json }) => {
                        json.push_str(&partial_json);
                    }
                    // Progressive image previews don't accumulate into a
                    // final content block; a caller that wants them observes
                    // the raw stream directly instead of going through this
                    // accumulator.
                    (_, Delta::PartialImage { .. }) => {}
                    (block, delta) => {
                        return Err(LlmError::Stream {
                            provider: "accumulate",
                            kind: ApiErrorKind::Other,
                            message: format!(
                                "delta {delta:?} does not match the block in progress at index {index}: {block:?}"
                            ),
                        });
                    }
                }
            }
            StreamEvent::BlockStop { .. } => {}
            StreamEvent::MessageStop { stop_reason, usage } => {
                self.stop_reason = Some(stop_reason);
                self.usage = usage;
            }
        }
        Ok(())
    }

    /// Finish accumulation. Errors if the stream never sent a `MessageStop`
    /// event, or if a tool call's accumulated JSON doesn't parse.
    pub fn finish(self) -> Result<CompletedMessage, LlmError> {
        let stop_reason = self.stop_reason.ok_or_else(|| LlmError::Stream {
            provider: "accumulate",
            kind: ApiErrorKind::Other,
            message: "stream ended without a message_stop event".to_string(),
        })?;

        let mut content = Vec::with_capacity(self.blocks.len());
        for (index, block) in self.blocks {
            content.push(match block {
                PartialBlock::Text(text) => ContentBlock::Text { text },
                PartialBlock::Thinking { thinking, signature } => {
                    ContentBlock::Thinking { thinking, signature }
                }
                PartialBlock::ToolUse { id, name, json } => {
                    let input = if json.trim().is_empty() {
                        serde_json::json!({})
                    } else {
                        serde_json::from_str(&json).map_err(|source| LlmError::Decode {
                            provider: "accumulate",
                            context: format!("tool_use input for block {index}"),
                            source,
                        })?
                    };
                    ContentBlock::ToolUse { id, name, input }
                }
                PartialBlock::Other(block) => block,
            });
        }

        Ok(CompletedMessage {
            role: Role::Assistant,
            content,
            stop_reason,
            usage: self.usage,
            model: self.model,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_delta(index: usize, text: &str) -> StreamEvent {
        StreamEvent::Delta {
            index,
            delta: Delta::Text {
                text: text.to_string(),
            },
        }
    }

    #[test]
    fn accumulates_text_deltas_into_a_single_block() {
        let mut acc = MessageAccumulator::new();
        acc.push(StreamEvent::MessageStart {
            model: "claude-opus-5".to_string(),
        })
        .unwrap();
        acc.push(StreamEvent::BlockStart {
            index: 0,
            block: ContentBlock::Text {
                text: String::new(),
            },
        })
        .unwrap();
        acc.push(text_delta(0, "Hello, ")).unwrap();
        acc.push(text_delta(0, "world!")).unwrap();
        acc.push(StreamEvent::BlockStop { index: 0 }).unwrap();
        acc.push(StreamEvent::MessageStop {
            stop_reason: StopReason::EndTurn,
            usage: Usage {
                input_tokens: 10,
                output_tokens: 5,
                ..Default::default()
            },
        })
        .unwrap();

        let message = acc.finish().unwrap();
        assert_eq!(message.model.as_deref(), Some("claude-opus-5"));
        assert_eq!(message.stop_reason, StopReason::EndTurn);
        assert_eq!(message.content.len(), 1);
        match &message.content[0] {
            ContentBlock::Text { text } => assert_eq!(text, "Hello, world!"),
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn accumulates_a_thinking_block_and_its_trailing_signature() {
        let mut acc = MessageAccumulator::new();
        acc.push(StreamEvent::BlockStart {
            index: 0,
            block: ContentBlock::Thinking {
                thinking: String::new(),
                signature: None,
            },
        })
        .unwrap();
        acc.push(StreamEvent::Delta {
            index: 0,
            delta: Delta::Thinking {
                thinking: "reasoning...".to_string(),
            },
        })
        .unwrap();
        acc.push(StreamEvent::Delta {
            index: 0,
            delta: Delta::Signature {
                signature: "sig-abc123".to_string(),
            },
        })
        .unwrap();
        acc.push(StreamEvent::MessageStop {
            stop_reason: StopReason::EndTurn,
            usage: Usage::default(),
        })
        .unwrap();

        let message = acc.finish().unwrap();
        match &message.content[0] {
            ContentBlock::Thinking { thinking, signature } => {
                assert_eq!(thinking, "reasoning...");
                assert_eq!(signature.as_deref(), Some("sig-abc123"));
            }
            other => panic!("expected Thinking, got {other:?}"),
        }
    }

    #[test]
    fn accumulates_tool_input_json_fragments_and_parses_them_at_the_end() {
        let mut acc = MessageAccumulator::new();
        acc.push(StreamEvent::BlockStart {
            index: 0,
            block: ContentBlock::ToolUse {
                id: "toolu_1".to_string(),
                name: "get_weather".to_string(),
                input: serde_json::Value::Null,
            },
        })
        .unwrap();
        acc.push(StreamEvent::Delta {
            index: 0,
            delta: Delta::ToolInputJson {
                partial_json: "{\"loc".to_string(),
            },
        })
        .unwrap();
        acc.push(StreamEvent::Delta {
            index: 0,
            delta: Delta::ToolInputJson {
                partial_json: "ation\": \"Paris\"}".to_string(),
            },
        })
        .unwrap();
        acc.push(StreamEvent::MessageStop {
            stop_reason: StopReason::ToolUse,
            usage: Usage::default(),
        })
        .unwrap();

        let message = acc.finish().unwrap();
        match &message.content[0] {
            ContentBlock::ToolUse { name, input, .. } => {
                assert_eq!(name, "get_weather");
                assert_eq!(input, &serde_json::json!({"location": "Paris"}));
            }
            other => panic!("expected ToolUse, got {other:?}"),
        }
    }

    #[test]
    fn interleaves_multiple_block_indices_in_order() {
        let mut acc = MessageAccumulator::new();
        acc.push(StreamEvent::BlockStart {
            index: 0,
            block: ContentBlock::Text {
                text: String::new(),
            },
        })
        .unwrap();
        acc.push(StreamEvent::BlockStart {
            index: 1,
            block: ContentBlock::ToolUse {
                id: "toolu_1".to_string(),
                name: "search".to_string(),
                input: serde_json::Value::Null,
            },
        })
        .unwrap();
        // Deltas for block 1 arrive before block 0 finishes — the
        // accumulator must not assume in-order-only delivery.
        acc.push(StreamEvent::Delta {
            index: 1,
            delta: Delta::ToolInputJson {
                partial_json: "{}".to_string(),
            },
        })
        .unwrap();
        acc.push(text_delta(0, "thinking out loud")).unwrap();
        acc.push(StreamEvent::MessageStop {
            stop_reason: StopReason::ToolUse,
            usage: Usage::default(),
        })
        .unwrap();

        let message = acc.finish().unwrap();
        assert_eq!(message.content.len(), 2);
        assert!(matches!(message.content[0], ContentBlock::Text { .. }));
        assert!(matches!(message.content[1], ContentBlock::ToolUse { .. }));
    }

    #[test]
    fn errors_if_message_stop_never_arrives() {
        let mut acc = MessageAccumulator::new();
        acc.push(text_delta(0, "incomplete")).unwrap();
        let err = acc.finish().unwrap_err();
        assert!(matches!(err, LlmError::Stream { .. }));
    }

    #[test]
    fn errors_on_mismatched_delta_shape() {
        let mut acc = MessageAccumulator::new();
        acc.push(StreamEvent::BlockStart {
            index: 0,
            block: ContentBlock::ToolUse {
                id: "toolu_1".to_string(),
                name: "search".to_string(),
                input: serde_json::Value::Null,
            },
        })
        .unwrap();
        let err = acc.push(text_delta(0, "oops")).unwrap_err();
        assert!(matches!(err, LlmError::Stream { .. }));
    }

    #[test]
    fn a_tool_use_block_with_no_input_deltas_defaults_to_an_empty_object() {
        let mut acc = MessageAccumulator::new();
        acc.push(StreamEvent::BlockStart {
            index: 0,
            block: ContentBlock::ToolUse {
                id: "toolu_1".to_string(),
                name: "ping".to_string(),
                input: serde_json::Value::Null,
            },
        })
        .unwrap();
        acc.push(StreamEvent::MessageStop {
            stop_reason: StopReason::ToolUse,
            usage: Usage::default(),
        })
        .unwrap();
        let message = acc.finish().unwrap();
        match &message.content[0] {
            ContentBlock::ToolUse { input, .. } => assert_eq!(input, &serde_json::json!({})),
            other => panic!("expected ToolUse, got {other:?}"),
        }
    }
}
