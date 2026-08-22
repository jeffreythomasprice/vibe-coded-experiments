//! Turns a conversation's stored messages, plus an in-flight turn's streamed
//! events, into the flat list of bubbles a view renders.
//!
//! Pure functions/state over `shared` types — no wasm, no I/O — so this is
//! covered by ordinary unit tests (`cargo test -p client`) per this repo's
//! preference for testing over running the app. See [`flatten`] for stored
//! history and [`Draft`] for an in-flight turn's live events.
//!
//! The one modeling wrinkle worth spelling out: [`shared::llm::message::Role`]
//! has only `User`/`Assistant` — the four message *formats* the UI
//! distinguishes (human / assistant / tool / thinking) are
//! [`shared::llm::message::ContentBlock`] variants, not roles. In particular a
//! tool result rides inside a `User`-role message (`lib::agent` appends the
//! executed results as `Message::user(completed_results)`), so a renderer
//! keyed on role alone would draw tool output as a human bubble.

use std::collections::HashMap;

use shared::agent::event::AgentEvent;
use shared::conversation::StoredMessage;
use shared::llm::image::ImageSource;
use shared::llm::message::{ContentBlock, Role, ToolResultContent};
use shared::llm::stream::{Delta, StreamEvent};

/// One bubble's content, independent of how it's laid out on screen.
#[derive(Debug, Clone, PartialEq)]
pub enum Bubble {
    Human { text: String },
    /// Raw markdown source — the view runs this through `markdown::render`.
    Assistant { markdown: String },
    Thinking { text: String, redacted: bool },
    Tool {
        name: String,
        input: serde_json::Value,
        /// `None` while the call hasn't finished (or, for a tool requiring
        /// approval, may never finish without a decision — approval isn't
        /// wired into this UI, so such a call just renders as pending).
        result: Option<ToolOutcome>,
    },
    Image { source: ImageSource },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolOutcome {
    pub is_error: bool,
    pub content: Vec<ToolResultContent>,
}

/// One bubble plus when it happened.
#[derive(Debug, Clone, PartialEq)]
pub struct Rendered {
    /// ISO8601 UTC from `StoredMessage::created_at`. `None` for a bubble
    /// still being streamed in by [`Draft`] — it has no row, and so no
    /// timestamp, until the turn settles and the view refetches.
    pub timestamp: Option<String>,
    pub bubble: Bubble,
}

/// Flatten a conversation's stored messages into one bubble per content
/// block, in `seq` order (never by timestamp — every message a single turn
/// writes shares one `created_at`, so `seq` is the only reliable order).
///
/// A `ToolResult` is attached to the `Tool` bubble opened by the matching
/// `ToolUse`'s id; one with no match (truncated history, a decision replayed
/// without its request) still renders, standalone.
pub fn flatten(messages: &[StoredMessage]) -> Vec<Rendered> {
    let mut rendered: Vec<Rendered> = Vec::new();
    let mut tool_positions: HashMap<&str, usize> = HashMap::new();

    for message in messages {
        for block in &message.content {
            match block {
                ContentBlock::Text { text } => {
                    let bubble = match message.role {
                        Role::User => Bubble::Human { text: text.clone() },
                        Role::Assistant => Bubble::Assistant { markdown: text.clone() },
                    };
                    rendered.push(Rendered {
                        timestamp: Some(message.created_at.clone()),
                        bubble,
                    });
                }
                ContentBlock::Thinking { thinking, .. } => {
                    rendered.push(Rendered {
                        timestamp: Some(message.created_at.clone()),
                        bubble: Bubble::Thinking {
                            text: thinking.clone(),
                            redacted: false,
                        },
                    });
                }
                ContentBlock::RedactedThinking { .. } => {
                    rendered.push(Rendered {
                        timestamp: Some(message.created_at.clone()),
                        bubble: Bubble::Thinking {
                            text: String::new(),
                            redacted: true,
                        },
                    });
                }
                ContentBlock::Image { source } => {
                    rendered.push(Rendered {
                        timestamp: Some(message.created_at.clone()),
                        bubble: Bubble::Image { source: source.clone() },
                    });
                }
                ContentBlock::ToolUse { id, name, input } => {
                    tool_positions.insert(id.as_str(), rendered.len());
                    rendered.push(Rendered {
                        timestamp: Some(message.created_at.clone()),
                        bubble: Bubble::Tool {
                            name: name.clone(),
                            input: input.clone(),
                            result: None,
                        },
                    });
                }
                ContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    is_error,
                } => {
                    let outcome = ToolOutcome {
                        is_error: *is_error,
                        content: content.clone(),
                    };
                    match tool_positions.get(tool_use_id.as_str()) {
                        Some(&position) => {
                            if let Bubble::Tool { result, .. } = &mut rendered[position].bubble {
                                *result = Some(outcome);
                            }
                        }
                        None => rendered.push(Rendered {
                            timestamp: Some(message.created_at.clone()),
                            bubble: Bubble::Tool {
                                name: "unknown".to_string(),
                                input: serde_json::Value::Null,
                                result: Some(outcome),
                            },
                        }),
                    }
                }
                ContentBlock::Unknown => {}
            }
        }
    }

    rendered
}

/// Folds an in-flight turn's `AgentEvent`s into the same [`Rendered`] shape
/// [`flatten`] produces, so a view can render persisted history and the
/// live turn as one continuous list. Construct fresh per turn.
#[derive(Debug, Default, Clone)]
pub struct Draft {
    bubbles: Vec<Rendered>,
    /// The block still accumulating deltas at each stream index, scoped to
    /// the current step — see [`Draft::apply`]'s `StepStart` arm. Block
    /// indices and usage are only meaningful within one provider response
    /// (`AgentEvent::Model`'s doc), so a stale entry from a prior step must
    /// never be matched against the next step's events.
    open: HashMap<usize, OpenBlock>,
    /// `tool_use_id` -> its bubble's position, so `ToolStart`/`ToolEnd`
    /// (keyed by id, not stream index) can find the bubble the model's own
    /// `ToolUse` block already opened.
    tool_positions: HashMap<String, usize>,
}

#[derive(Debug, Clone)]
enum OpenBlock {
    Assistant { position: usize },
    Thinking { position: usize },
    Tool { position: usize, partial_json: String },
    /// A block kind with no deltas this view renders incrementally (an
    /// image, a tool result, or a redacted-thinking shell) — tracked only so
    /// `BlockStop` has an entry to remove.
    Opaque,
}

impl Draft {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply(&mut self, event: &AgentEvent) {
        match event {
            AgentEvent::StepStart { .. } => self.open.clear(),
            AgentEvent::Model { event, .. } => self.apply_stream_event(event),
            AgentEvent::ToolStart { tool_use_id, name, input } => {
                if !self.tool_positions.contains_key(tool_use_id) {
                    let position = self.push(Bubble::Tool {
                        name: name.clone(),
                        input: input.clone(),
                        result: None,
                    });
                    self.tool_positions.insert(tool_use_id.clone(), position);
                }
            }
            AgentEvent::ToolEnd {
                tool_use_id,
                name,
                result,
            } => {
                let outcome = tool_outcome(result);
                match self.tool_positions.get(tool_use_id) {
                    Some(&position) => {
                        if let Bubble::Tool { result, .. } = &mut self.bubbles[position].bubble {
                            *result = Some(outcome);
                        }
                    }
                    None => {
                        let position = self.push(Bubble::Tool {
                            name: name.clone(),
                            input: serde_json::Value::Null,
                            result: Some(outcome),
                        });
                        self.tool_positions.insert(tool_use_id.clone(), position);
                    }
                }
            }
            // Terminal marker only — the caller's own await on the command
            // future already knows when to refetch and drop this draft.
            AgentEvent::Turn(_) => {}
        }
    }

    fn apply_stream_event(&mut self, event: &StreamEvent) {
        match event {
            StreamEvent::MessageStart { .. } | StreamEvent::MessageStop { .. } => {}
            StreamEvent::BlockStart { index, block } => {
                let open = match block {
                    ContentBlock::Text { text } => OpenBlock::Assistant {
                        position: self.push(Bubble::Assistant { markdown: text.clone() }),
                    },
                    ContentBlock::Thinking { thinking, .. } => OpenBlock::Thinking {
                        position: self.push(Bubble::Thinking {
                            text: thinking.clone(),
                            redacted: false,
                        }),
                    },
                    ContentBlock::RedactedThinking { .. } => {
                        self.push(Bubble::Thinking {
                            text: String::new(),
                            redacted: true,
                        });
                        OpenBlock::Opaque
                    }
                    ContentBlock::ToolUse { id, name, input } => {
                        let position = self.push(Bubble::Tool {
                            name: name.clone(),
                            input: input.clone(),
                            result: None,
                        });
                        self.tool_positions.insert(id.clone(), position);
                        OpenBlock::Tool {
                            position,
                            partial_json: String::new(),
                        }
                    }
                    ContentBlock::Image { source } => {
                        self.push(Bubble::Image { source: source.clone() });
                        OpenBlock::Opaque
                    }
                    ContentBlock::ToolResult { .. } | ContentBlock::Unknown => OpenBlock::Opaque,
                };
                self.open.insert(*index, open);
            }
            StreamEvent::Delta { index, delta } => {
                let Some(open) = self.open.get_mut(index) else {
                    return;
                };
                match (open, delta) {
                    (OpenBlock::Assistant { position }, Delta::Text { text }) => {
                        if let Bubble::Assistant { markdown } = &mut self.bubbles[*position].bubble {
                            markdown.push_str(text);
                        }
                    }
                    (OpenBlock::Thinking { position }, Delta::Thinking { thinking }) => {
                        if let Bubble::Thinking { text, .. } = &mut self.bubbles[*position].bubble {
                            text.push_str(thinking);
                        }
                    }
                    (OpenBlock::Tool { partial_json, .. }, Delta::ToolInputJson { partial_json: chunk }) => {
                        partial_json.push_str(chunk);
                    }
                    // A thinking block's trailing signature must replay
                    // byte-for-byte on a real send, but has nothing to show;
                    // a partial image frame isn't rendered incrementally here.
                    _ => {}
                }
            }
            StreamEvent::BlockStop { index } => {
                if let Some(OpenBlock::Tool { position, partial_json }) = self.open.remove(index) {
                    if !partial_json.is_empty() {
                        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&partial_json) {
                            if let Bubble::Tool { input, .. } = &mut self.bubbles[position].bubble {
                                *input = value;
                            }
                        }
                    }
                }
            }
        }
    }

    fn push(&mut self, bubble: Bubble) -> usize {
        let position = self.bubbles.len();
        self.bubbles.push(Rendered { timestamp: None, bubble });
        position
    }

    pub fn bubbles(&self) -> Vec<Rendered> {
        self.bubbles.clone()
    }

    pub fn is_empty(&self) -> bool {
        self.bubbles.is_empty()
    }
}

fn tool_outcome(result: &ContentBlock) -> ToolOutcome {
    match result {
        ContentBlock::ToolResult { content, is_error, .. } => ToolOutcome {
            is_error: *is_error,
            content: content.clone(),
        },
        // ToolEnd's doc guarantees a ContentBlock::ToolResult; this arm only
        // exists so the match stays exhaustive against ContentBlock's other
        // variants, not because it's expected in practice.
        _ => ToolOutcome {
            is_error: true,
            content: vec![ToolResultContent::Text {
                text: "malformed tool result".to_string(),
            }],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::ids::{MessageId, TurnId};
    use shared::llm::message::StopReason;
    use shared::llm::stream::StreamEvent;

    fn message(seq: u32, role: Role, content: Vec<ContentBlock>, created_at: &str) -> StoredMessage {
        StoredMessage {
            id: MessageId(seq as i64 + 1),
            seq,
            turn_id: Some(TurnId(1)),
            role,
            content,
            created_at: created_at.to_string(),
        }
    }

    #[test]
    fn flatten_orders_and_shapes_every_block_kind() {
        let messages = vec![
            message(
                0,
                Role::User,
                vec![ContentBlock::Text {
                    text: "deploy it".to_string(),
                }],
                "2026-08-22T00:00:00.000Z",
            ),
            message(
                1,
                Role::Assistant,
                vec![
                    ContentBlock::Thinking {
                        thinking: "let me check the runbook".to_string(),
                        signature: Some("sig".to_string()),
                    },
                    ContentBlock::Text {
                        text: "On it.".to_string(),
                    },
                    ContentBlock::ToolUse {
                        id: "call_1_0".to_string(),
                        name: "deploy".to_string(),
                        input: serde_json::json!({"env": "prod"}),
                    },
                ],
                "2026-08-22T00:00:01.000Z",
            ),
            message(
                2,
                Role::User,
                vec![ContentBlock::ToolResult {
                    tool_use_id: "call_1_0".to_string(),
                    content: vec![ToolResultContent::Text {
                        text: "deployed".to_string(),
                    }],
                    is_error: false,
                }],
                "2026-08-22T00:00:01.000Z",
            ),
        ];

        let bubbles = flatten(&messages);
        assert_eq!(bubbles.len(), 4, "{bubbles:?}");

        assert_eq!(
            bubbles[0],
            Rendered {
                timestamp: Some("2026-08-22T00:00:00.000Z".to_string()),
                bubble: Bubble::Human {
                    text: "deploy it".to_string()
                },
            }
        );
        assert!(matches!(&bubbles[1].bubble, Bubble::Thinking { redacted: false, .. }));
        assert_eq!(
            bubbles[2].bubble,
            Bubble::Assistant {
                markdown: "On it.".to_string()
            }
        );
        match &bubbles[3].bubble {
            Bubble::Tool { name, input, result } => {
                assert_eq!(name, "deploy");
                assert_eq!(input["env"], serde_json::json!("prod"));
                let outcome = result.as_ref().expect("the tool result should have attached");
                assert!(!outcome.is_error);
                assert_eq!(
                    outcome.content,
                    vec![ToolResultContent::Text {
                        text: "deployed".to_string()
                    }]
                );
            }
            other => panic!("expected a Tool bubble, got {other:?}"),
        }
    }

    #[test]
    fn a_tool_result_with_no_matching_tool_use_still_renders() {
        let messages = vec![message(
            0,
            Role::User,
            vec![ContentBlock::ToolResult {
                tool_use_id: "call_missing".to_string(),
                content: vec![],
                is_error: true,
            }],
            "2026-08-22T00:00:00.000Z",
        )];

        let bubbles = flatten(&messages);
        assert_eq!(bubbles.len(), 1);
        match &bubbles[0].bubble {
            Bubble::Tool { name, result, .. } => {
                assert_eq!(name, "unknown");
                assert!(result.as_ref().unwrap().is_error);
            }
            other => panic!("expected a standalone Tool bubble, got {other:?}"),
        }
    }

    #[test]
    fn redacted_thinking_renders_with_no_text() {
        let messages = vec![message(
            0,
            Role::Assistant,
            vec![ContentBlock::RedactedThinking {
                data: "opaque".to_string(),
            }],
            "2026-08-22T00:00:00.000Z",
        )];
        let bubbles = flatten(&messages);
        assert_eq!(
            bubbles[0].bubble,
            Bubble::Thinking {
                text: String::new(),
                redacted: true,
            }
        );
    }

    fn model_event(step: u32, event: StreamEvent) -> AgentEvent {
        AgentEvent::Model { step, event }
    }

    #[test]
    fn draft_streams_assistant_text_in_incrementally_across_deltas() {
        let mut draft = Draft::new();
        draft.apply(&AgentEvent::StepStart { step: 0 });
        draft.apply(&model_event(
            0,
            StreamEvent::BlockStart {
                index: 0,
                block: ContentBlock::Text { text: String::new() },
            },
        ));
        assert_eq!(
            draft.bubbles()[0].bubble,
            Bubble::Assistant { markdown: String::new() },
            "the bubble must appear as soon as the block opens, before any delta"
        );

        draft.apply(&model_event(
            0,
            StreamEvent::Delta {
                index: 0,
                delta: Delta::Text { text: "Hel".to_string() },
            },
        ));
        draft.apply(&model_event(
            0,
            StreamEvent::Delta {
                index: 0,
                delta: Delta::Text { text: "lo".to_string() },
            },
        ));
        assert_eq!(
            draft.bubbles()[0].bubble,
            Bubble::Assistant {
                markdown: "Hello".to_string()
            }
        );

        draft.apply(&model_event(0, StreamEvent::BlockStop { index: 0 }));
        assert_eq!(draft.bubbles().len(), 1);
        assert!(draft.bubbles()[0].timestamp.is_none(), "a streamed bubble has no row yet");
    }

    #[test]
    fn draft_step_start_resets_indices_so_a_reused_index_does_not_corrupt_the_prior_step() {
        let mut draft = Draft::new();

        // Step 0: assistant text at index 0.
        draft.apply(&AgentEvent::StepStart { step: 0 });
        draft.apply(&model_event(
            0,
            StreamEvent::BlockStart {
                index: 0,
                block: ContentBlock::Text { text: String::new() },
            },
        ));
        draft.apply(&model_event(
            0,
            StreamEvent::Delta {
                index: 0,
                delta: Delta::Text { text: "Hello".to_string() },
            },
        ));
        draft.apply(&model_event(0, StreamEvent::BlockStop { index: 0 }));

        // Step 1 reuses index 0 for an unrelated tool call.
        draft.apply(&AgentEvent::StepStart { step: 1 });
        draft.apply(&model_event(
            1,
            StreamEvent::BlockStart {
                index: 0,
                block: ContentBlock::ToolUse {
                    id: "call_1_0".to_string(),
                    name: "deploy".to_string(),
                    input: serde_json::json!({}),
                },
            },
        ));
        draft.apply(&model_event(
            1,
            StreamEvent::Delta {
                index: 0,
                delta: Delta::ToolInputJson {
                    partial_json: r#"{"env":"prod"}"#.to_string(),
                },
            },
        ));
        draft.apply(&model_event(1, StreamEvent::BlockStop { index: 0 }));

        let bubbles = draft.bubbles();
        assert_eq!(bubbles.len(), 2, "{bubbles:?}");
        assert_eq!(
            bubbles[0].bubble,
            Bubble::Assistant {
                markdown: "Hello".to_string()
            },
            "step 0's bubble must survive untouched"
        );
        match &bubbles[1].bubble {
            Bubble::Tool { name, input, .. } => {
                assert_eq!(name, "deploy");
                assert_eq!(input["env"], serde_json::json!("prod"));
            }
            other => panic!("expected a Tool bubble, got {other:?}"),
        }
    }

    #[test]
    fn tool_start_and_tool_end_fill_in_the_bubble_the_model_stream_opened() {
        let mut draft = Draft::new();
        draft.apply(&AgentEvent::StepStart { step: 0 });
        draft.apply(&model_event(
            0,
            StreamEvent::BlockStart {
                index: 0,
                block: ContentBlock::ToolUse {
                    id: "call_1_0".to_string(),
                    name: "deploy".to_string(),
                    input: serde_json::json!({}),
                },
            },
        ));
        draft.apply(&model_event(0, StreamEvent::BlockStop { index: 0 }));

        draft.apply(&AgentEvent::ToolStart {
            tool_use_id: "call_1_0".to_string(),
            name: "deploy".to_string(),
            input: serde_json::json!({}),
        });
        assert_eq!(draft.bubbles().len(), 1, "ToolStart must reuse the existing bubble");

        draft.apply(&AgentEvent::ToolEnd {
            tool_use_id: "call_1_0".to_string(),
            name: "deploy".to_string(),
            result: ContentBlock::ToolResult {
                tool_use_id: "call_1_0".to_string(),
                content: vec![ToolResultContent::Text {
                    text: "deployed".to_string(),
                }],
                is_error: false,
            },
        });

        let bubbles = draft.bubbles();
        assert_eq!(bubbles.len(), 1);
        match &bubbles[0].bubble {
            Bubble::Tool { result: Some(outcome), .. } => assert!(!outcome.is_error),
            other => panic!("expected a completed Tool bubble, got {other:?}"),
        }
    }

    #[test]
    fn turn_event_is_a_no_op_on_the_draft() {
        let mut draft = Draft::new();
        draft.apply(&AgentEvent::Turn(shared::agent::AgentTurn {
            conversation: Default::default(),
            added: vec![],
            usage: Default::default(),
            steps: 1,
            stop: shared::agent::TurnStop::Done {
                stop_reason: StopReason::EndTurn,
            },
        }));
        assert!(draft.is_empty());
    }
}
