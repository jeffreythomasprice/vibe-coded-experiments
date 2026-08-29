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

use std::collections::{HashMap, HashSet};

use shared::agent::event::AgentEvent;
use shared::agent::{DecidedBy, Decision};
use shared::conversation::{StoredMessage, ToolDecisionView};
use shared::ids::TurnId;
use shared::llm::image::ImageSource;
use shared::llm::message::{ContentBlock, Role, ToolResultContent};
use shared::llm::stream::{Delta, StreamEvent};

/// One bubble's content, independent of how it's laid out on screen.
#[derive(Debug, Clone, PartialEq)]
pub enum Bubble {
    /// Raw markdown source — the view runs this through
    /// `markdown::render_with_breaks`, so a composer newline lands as a
    /// line break rather than CommonMark's soft-break reflow.
    Human { text: String },
    /// Raw markdown source — the view runs this through `markdown::render`.
    Assistant { markdown: String },
    Thinking { text: String, redacted: bool },
    Tool {
        /// What an Approve/Deny click, or a decision lookup, targets. Not
        /// present on every historical bubble the same way (a very old
        /// conversation predates nothing here, but a *replayed* tool result
        /// with no matching `ToolUse` has no real id to show) — carried
        /// anyway since every live and stored path this crate builds one
        /// from does have it.
        tool_use_id: String,
        name: String,
        input: serde_json::Value,
        /// `None` while the call hasn't finished — including one still
        /// awaiting approval (`awaiting` is what distinguishes that case
        /// from an ordinary in-flight call).
        result: Option<ToolOutcome>,
        /// Who resolved a gated call, and when — `None` for an automatic
        /// call, or a gated one nobody has decided yet.
        decision: Option<BubbleDecision>,
        /// True for a call from `PendingApproval::requests` with no
        /// decision yet — the view renders Approve/Deny controls for these
        /// instead of a settled outcome.
        awaiting: bool,
    },
    Image { source: ImageSource },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolOutcome {
    pub is_error: bool,
    pub content: Vec<ToolResultContent>,
}

/// Provenance for one gated call's resolution, attached to its [`Bubble::Tool`]
/// — the display counterpart of [`ToolDecisionView`], which is where a
/// settled decision comes from on reload, and of the live
/// [`AgentEvent::ToolDecided`] event, which is where one comes from while
/// still streaming (and so carries no `decided_at` yet).
#[derive(Debug, Clone, PartialEq)]
pub struct BubbleDecision {
    pub decision: Decision,
    pub decided_by: Option<DecidedBy>,
    pub decided_at: Option<String>,
}

impl From<&ToolDecisionView> for BubbleDecision {
    fn from(view: &ToolDecisionView) -> Self {
        Self {
            decision: view.decision.clone(),
            decided_by: view.decided_by.clone(),
            decided_at: view.decided_at.clone(),
        }
    }
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
///
/// `decisions` supplies provenance for gated calls, keyed by
/// **`(turn_id, tool_use_id)`**, never `tool_use_id` alone —
/// `lib::agent::rewrite_tool_use_ids` restarts its counter every turn, so
/// the same id recurs across a conversation's turns and a bare-id lookup
/// would cross-match a decision from one turn onto an unrelated call in
/// another. `awaiting` marks the bare `tool_use_id`s (unambiguous here,
/// since these always come from one specific still-suspended turn) that
/// have no decision yet — ordinary callers pass an empty set; only
/// [`flatten_pending`] passes a non-empty one.
pub fn flatten(messages: &[StoredMessage], decisions: &[ToolDecisionView], awaiting: &HashSet<&str>) -> Vec<Rendered> {
    let mut rendered: Vec<Rendered> = Vec::new();
    let mut tool_positions: HashMap<&str, usize> = HashMap::new();
    let decision_by_id: HashMap<(Option<TurnId>, &str), BubbleDecision> = decisions
        .iter()
        .map(|d| ((Some(d.turn_id), d.tool_use_id.as_str()), BubbleDecision::from(d)))
        .collect();

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
                    let decision = decision_by_id.get(&(message.turn_id, id.as_str())).cloned();
                    rendered.push(Rendered {
                        timestamp: Some(message.created_at.clone()),
                        bubble: Bubble::Tool {
                            tool_use_id: id.clone(),
                            name: name.clone(),
                            input: input.clone(),
                            result: None,
                            decision,
                            awaiting: awaiting.contains(id.as_str()),
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
                        None => {
                            let decision = decision_by_id.get(&(message.turn_id, tool_use_id.as_str())).cloned();
                            rendered.push(Rendered {
                                timestamp: Some(message.created_at.clone()),
                                bubble: Bubble::Tool {
                                    tool_use_id: tool_use_id.clone(),
                                    name: "unknown".to_string(),
                                    input: serde_json::Value::Null,
                                    result: Some(outcome),
                                    decision,
                                    awaiting: false,
                                },
                            })
                        }
                    }
                }
                ContentBlock::Unknown => {}
            }
        }
    }

    rendered
}

/// Renders a suspended turn's tail as the same [`Rendered`] shape [`flatten`]
/// produces: the same-step calls that already ran (`completed` — shown so
/// the user can see what happened before deciding on the rest) plus the
/// call(s) still awaiting a decision (`requests`). Neither has a real row
/// yet, so every bubble here streams-styled (`timestamp: None`), the same
/// way a still-streaming [`Draft`] bubble is.
///
/// Stamps every synthesized message with `pending.turn_id` — the id
/// `decisions` (see [`flatten`]'s doc) is keyed by — so a policy decision
/// made in the very step that suspended the turn (already visible in
/// `ConversationView.decisions` by the time this renders; `settle` writes
/// both in the same transaction) attaches correctly.
pub fn flatten_pending(pending: &shared::conversation::PendingApproval, decisions: &[ToolDecisionView]) -> Vec<Rendered> {
    let mut fake: Vec<StoredMessage> = pending
        .added
        .iter()
        .enumerate()
        .map(|(index, message)| StoredMessage {
            id: shared::ids::MessageId(0),
            seq: index as u32,
            turn_id: Some(pending.turn_id),
            role: message.role,
            content: message.content.clone(),
            created_at: String::new(),
        })
        .collect();
    if !pending.completed.is_empty() {
        fake.push(StoredMessage {
            id: shared::ids::MessageId(0),
            seq: fake.len() as u32,
            turn_id: Some(pending.turn_id),
            role: Role::User,
            content: pending.completed.clone(),
            created_at: String::new(),
        });
    }

    let awaiting: HashSet<&str> = pending.requests.iter().map(|r| r.tool_use_id.as_str()).collect();
    flatten(&fake, decisions, &awaiting)
        .into_iter()
        .map(|mut rendered| {
            // These never had a real timestamp (`created_at: String::new()`
            // above) — show them the same way a still-streaming `Draft`
            // bubble is shown, rather than the empty string `flatten` would
            // otherwise carry through.
            rendered.timestamp = None;
            rendered
        })
        .collect()
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
    /// A `ToolDecided` event's decision, held here when it arrives — as it
    /// always does on the policy and resumed-user paths — *before* the
    /// `ToolStart` that opens the bubble it's about. Consumed (and removed)
    /// the moment that `ToolStart` arrives.
    pending_decisions: HashMap<String, BubbleDecision>,
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
            // Always precedes the `ToolStart`/`ToolEnd` pair it explains
            // (both the policy and resumed-user paths yield it first — see
            // `lib::agent::mod`'s `run_stream`) — so the bubble it's about
            // doesn't exist yet. Held in `pending_decisions` until that
            // `ToolStart` arrives and claims it.
            AgentEvent::ToolDecided { tool_use_id, decision, decided_by, .. } => {
                let bubble_decision = BubbleDecision {
                    decision: decision.clone(),
                    decided_by: Some(decided_by.clone()),
                    decided_at: None,
                };
                match self.tool_positions.get(tool_use_id) {
                    Some(&position) => {
                        if let Bubble::Tool { decision, .. } = &mut self.bubbles[position].bubble {
                            *decision = Some(bubble_decision);
                        }
                    }
                    None => {
                        self.pending_decisions.insert(tool_use_id.clone(), bubble_decision);
                    }
                }
            }
            AgentEvent::ToolStart { tool_use_id, name, input } => {
                if !self.tool_positions.contains_key(tool_use_id) {
                    let decision = self.pending_decisions.remove(tool_use_id);
                    let position = self.push(Bubble::Tool {
                        tool_use_id: tool_use_id.clone(),
                        name: name.clone(),
                        input: input.clone(),
                        result: None,
                        decision,
                        awaiting: false,
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
                        let decision = self.pending_decisions.remove(tool_use_id);
                        let position = self.push(Bubble::Tool {
                            tool_use_id: tool_use_id.clone(),
                            name: name.clone(),
                            input: serde_json::Value::Null,
                            result: Some(outcome),
                            decision,
                            awaiting: false,
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
                        let decision = self.pending_decisions.remove(id);
                        let position = self.push(Bubble::Tool {
                            tool_use_id: id.clone(),
                            name: name.clone(),
                            input: input.clone(),
                            result: None,
                            decision,
                            awaiting: false,
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

    /// No decisions, nothing awaiting — what most tests below want.
    fn flatten_plain(messages: &[StoredMessage]) -> Vec<Rendered> {
        flatten(messages, &[], &HashSet::new())
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

        let bubbles = flatten_plain(&messages);
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
            Bubble::Tool { tool_use_id, name, input, result, decision, awaiting } => {
                assert_eq!(tool_use_id, "call_1_0");
                assert_eq!(name, "deploy");
                assert_eq!(input["env"], serde_json::json!("prod"));
                assert!(decision.is_none());
                assert!(!awaiting);
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

        let bubbles = flatten_plain(&messages);
        assert_eq!(bubbles.len(), 1);
        match &bubbles[0].bubble {
            Bubble::Tool { name, result, .. } => {
                assert_eq!(name, "unknown");
                assert!(result.as_ref().unwrap().is_error);
            }
            other => panic!("expected a standalone Tool bubble, got {other:?}"),
        }
    }

    fn decision_view(turn_id: i64, tool_use_id: &str, decision: Decision, decided_by: Option<DecidedBy>) -> ToolDecisionView {
        ToolDecisionView {
            turn_id: TurnId(turn_id),
            tool_use_id: tool_use_id.to_string(),
            decision,
            decided_by,
            decided_at: Some("2026-08-29T00:00:00.000Z".to_string()),
        }
    }

    #[test]
    fn flatten_attaches_a_decision_matching_both_turn_id_and_tool_use_id() {
        let messages = vec![message(
            0,
            Role::Assistant,
            vec![ContentBlock::ToolUse {
                id: "call_1_0".to_string(),
                name: "deploy".to_string(),
                input: serde_json::json!({}),
            }],
            "2026-08-22T00:00:00.000Z",
        )];
        let decisions = vec![decision_view(1, "call_1_0", Decision::Approve, Some(DecidedBy::User))];

        let bubbles = flatten(&messages, &decisions, &HashSet::new());
        match &bubbles[0].bubble {
            Bubble::Tool { decision, .. } => {
                let decision = decision.as_ref().expect("expected a decision to attach");
                assert_eq!(decision.decision, Decision::Approve);
                assert_eq!(decision.decided_by, Some(DecidedBy::User));
            }
            other => panic!("expected a Tool bubble, got {other:?}"),
        }
    }

    /// The regression test for the exact hazard `lib::agent::rewrite_tool_use_ids`
    /// creates: `call_1_0` recurs in every turn, so a decision from one turn
    /// must never attach to another turn's call sharing that bare id.
    #[test]
    fn flatten_does_not_cross_match_a_decision_from_a_different_turn() {
        let messages = vec![message(
            0,
            Role::Assistant,
            vec![ContentBlock::ToolUse {
                id: "call_1_0".to_string(),
                name: "deploy".to_string(),
                input: serde_json::json!({}),
            }],
            "2026-08-22T00:00:00.000Z",
        )];
        // A decision for the *same* tool_use_id, but a different turn.
        let decisions = vec![decision_view(999, "call_1_0", Decision::Approve, Some(DecidedBy::User))];

        let bubbles = flatten(&messages, &decisions, &HashSet::new());
        match &bubbles[0].bubble {
            Bubble::Tool { decision, .. } => {
                assert!(decision.is_none(), "must not cross-match a decision from an unrelated turn");
            }
            other => panic!("expected a Tool bubble, got {other:?}"),
        }
    }

    #[test]
    fn flatten_marks_a_bubble_awaiting_when_its_id_is_in_the_awaiting_set() {
        let messages = vec![message(
            0,
            Role::Assistant,
            vec![ContentBlock::ToolUse {
                id: "call_1_0".to_string(),
                name: "deploy".to_string(),
                input: serde_json::json!({}),
            }],
            "2026-08-22T00:00:00.000Z",
        )];
        let awaiting: HashSet<&str> = ["call_1_0"].into_iter().collect();

        let bubbles = flatten(&messages, &[], &awaiting);
        match &bubbles[0].bubble {
            Bubble::Tool { awaiting, result, .. } => {
                assert!(*awaiting);
                assert!(result.is_none());
            }
            other => panic!("expected a Tool bubble, got {other:?}"),
        }
    }

    fn pending_approval(turn_id: i64, added: Vec<shared::llm::message::Message>, completed: Vec<ContentBlock>, requests: Vec<shared::agent::ToolApprovalRequest>) -> shared::conversation::PendingApproval {
        shared::conversation::PendingApproval {
            turn_id: TurnId(turn_id),
            added,
            requests,
            completed,
            usage: Default::default(),
            steps: 1,
        }
    }

    #[test]
    fn flatten_pending_renders_the_still_pending_call_as_awaiting_with_no_result() {
        use shared::agent::ToolApprovalRequest;
        use shared::llm::message::Message;

        let assistant = Message::assistant(vec![ContentBlock::ToolUse {
            id: "call_1_0".to_string(),
            name: "deploy".to_string(),
            input: serde_json::json!({"env": "prod"}),
        }]);
        let pending = pending_approval(
            1,
            vec![assistant],
            vec![],
            vec![ToolApprovalRequest {
                tool_use_id: "call_1_0".to_string(),
                name: "deploy".to_string(),
                input: serde_json::json!({"env": "prod"}),
            }],
        );

        let bubbles = flatten_pending(&pending, &[]);
        assert_eq!(bubbles.len(), 1);
        assert!(bubbles[0].timestamp.is_none(), "a pending bubble has no real row yet");
        match &bubbles[0].bubble {
            Bubble::Tool { tool_use_id, awaiting, result, .. } => {
                assert_eq!(tool_use_id, "call_1_0");
                assert!(*awaiting);
                assert!(result.is_none());
            }
            other => panic!("expected a Tool bubble, got {other:?}"),
        }
    }

    /// `completed` is exactly the same-step calls that already ran
    /// automatically alongside the still-gated one(s) — it must render too,
    /// with its outcome, not just the still-pending request.
    #[test]
    fn flatten_pending_renders_completed_calls_alongside_the_still_awaiting_one() {
        use shared::agent::ToolApprovalRequest;
        use shared::llm::message::Message;

        let assistant = Message::assistant(vec![
            ContentBlock::ToolUse {
                id: "call_1_0".to_string(),
                name: "get_weather".to_string(),
                input: serde_json::json!({}),
            },
            ContentBlock::ToolUse {
                id: "call_1_1".to_string(),
                name: "deploy".to_string(),
                input: serde_json::json!({}),
            },
        ]);
        let completed = vec![ContentBlock::ToolResult {
            tool_use_id: "call_1_0".to_string(),
            content: vec![ToolResultContent::Text { text: "sunny".to_string() }],
            is_error: false,
        }];
        let requests = vec![ToolApprovalRequest {
            tool_use_id: "call_1_1".to_string(),
            name: "deploy".to_string(),
            input: serde_json::json!({}),
        }];
        let pending = pending_approval(1, vec![assistant], completed, requests);

        let bubbles = flatten_pending(&pending, &[]);
        assert_eq!(bubbles.len(), 2, "{bubbles:?}");
        match &bubbles[0].bubble {
            Bubble::Tool { name, result, awaiting, .. } => {
                assert_eq!(name, "get_weather");
                assert!(result.is_some(), "the automatic call already ran and must show its outcome");
                assert!(!awaiting);
            }
            other => panic!("expected the completed call's bubble, got {other:?}"),
        }
        match &bubbles[1].bubble {
            Bubble::Tool { name, result, awaiting, .. } => {
                assert_eq!(name, "deploy");
                assert!(result.is_none());
                assert!(*awaiting);
            }
            other => panic!("expected the still-pending call's bubble, got {other:?}"),
        }
    }

    /// A policy decision made in the very step that suspended the turn is
    /// already visible in `ConversationView.decisions` by the time this
    /// renders (`settle` writes both in the same transaction) — so a
    /// `completed` call decided by policy shows its provenance too.
    #[test]
    fn flatten_pending_attaches_provenance_to_a_policy_decided_completed_call() {
        use shared::llm::message::Message;

        let assistant = Message::assistant(vec![ContentBlock::ToolUse {
            id: "call_1_0".to_string(),
            name: "deploy".to_string(),
            input: serde_json::json!({}),
        }]);
        let completed = vec![ContentBlock::ToolResult {
            tool_use_id: "call_1_0".to_string(),
            content: vec![ToolResultContent::Text { text: "deployed".to_string() }],
            is_error: false,
        }];
        let pending = pending_approval(1, vec![assistant], completed, vec![]);
        let decisions = vec![decision_view(
            1,
            "call_1_0",
            Decision::Approve,
            Some(DecidedBy::Policy { reason: "matched auto-approve rule".to_string() }),
        )];

        let bubbles = flatten_pending(&pending, &decisions);
        match &bubbles[0].bubble {
            Bubble::Tool { decision, .. } => {
                let decision = decision.as_ref().expect("expected the policy decision to attach");
                match &decision.decided_by {
                    Some(DecidedBy::Policy { reason }) => assert_eq!(reason, "matched auto-approve rule"),
                    other => panic!("expected DecidedBy::Policy, got {other:?}"),
                }
            }
            other => panic!("expected a Tool bubble, got {other:?}"),
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
        let bubbles = flatten_plain(&messages);
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

    /// `ToolDecided` always precedes the `ToolStart` it explains (both the
    /// policy and resumed-user paths yield it first) — so the bubble it's
    /// about doesn't exist yet when it arrives. Regression test for the
    /// hazard that would otherwise leave a policy-denied call spinning
    /// "running…" forever: `ToolStart`/`ToolEnd` must still follow, exactly
    /// as the automatic path does.
    #[test]
    fn tool_decided_before_tool_start_still_attaches_once_the_bubble_opens() {
        let mut draft = Draft::new();
        draft.apply(&AgentEvent::ToolDecided {
            tool_use_id: "call_1_0".to_string(),
            name: "deploy".to_string(),
            decision: Decision::Deny { reason: Some("matches auto-deny rule".to_string()) },
            decided_by: DecidedBy::Policy { reason: "matches auto-deny rule".to_string() },
        });
        assert!(draft.is_empty(), "no bubble exists yet — the decision is held, not lost");

        draft.apply(&AgentEvent::ToolStart {
            tool_use_id: "call_1_0".to_string(),
            name: "deploy".to_string(),
            input: serde_json::json!({}),
        });
        let bubbles = draft.bubbles();
        assert_eq!(bubbles.len(), 1);
        match &bubbles[0].bubble {
            Bubble::Tool { decision: Some(d), result, .. } => {
                assert_eq!(d.decision, Decision::Deny { reason: Some("matches auto-deny rule".to_string()) });
                assert!(matches!(&d.decided_by, Some(DecidedBy::Policy { .. })));
                assert!(result.is_none(), "ToolStart alone must not fill in a result");
            }
            other => panic!("expected a Tool bubble carrying the decision, got {other:?}"),
        }

        draft.apply(&AgentEvent::ToolEnd {
            tool_use_id: "call_1_0".to_string(),
            name: "deploy".to_string(),
            result: ContentBlock::ToolResult {
                tool_use_id: "call_1_0".to_string(),
                content: vec![ToolResultContent::Text {
                    text: "a policy denied this tool call: matches auto-deny rule".to_string(),
                }],
                is_error: true,
            },
        });
        let bubbles = draft.bubbles();
        assert_eq!(bubbles.len(), 1, "ToolEnd must reuse the same bubble ToolDecided/ToolStart opened");
        match &bubbles[0].bubble {
            Bubble::Tool { result: Some(outcome), decision: Some(_), .. } => assert!(outcome.is_error),
            other => panic!("expected the bubble to now carry both its decision and its result, got {other:?}"),
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
            decisions: vec![],
        }));
        assert!(draft.is_empty());
    }
}
