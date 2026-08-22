//! An agent: an exact model (via `shared::llm::ModelRef` and
//! `lib::llm::router::Router`), a system prompt, and a set of tools — plus
//! the loop that drives a conversation through it, executing tools and
//! pausing for the user's approval where a tool demands it.
//!
//! Build one with [`Agent::builder`], then drive it with [`Agent::next_turn`]
//! / [`Agent::send`] (blocking) or [`Agent::stream_turn`] (streaming). See
//! this module's tests for the full loop, tool-approval, and
//! blocking-vs-streaming-equivalence behavior, all exercised against
//! `lib::llm::testing::ScriptedProvider` — no network involved.

pub mod error;
pub mod tool;

pub use error::AgentError;
pub use tool::{json_tool, schema_for, tool, FnTool, JsonSchema, Tool, ToolBox, ToolOutput};

use std::collections::{BTreeSet, HashMap};
use std::pin::Pin;
use std::sync::Arc;

use futures_util::{Stream, StreamExt};
use shared::agent::{
    AgentEvent, AgentSpec, AgentTurn, Approval, Decision, ToolApprovalRequest, ToolDecision,
    TurnStop,
};
use shared::llm::{
    ChatOptions, ContentBlock, Conversation, Message, ModelRef, StopReason, Thinking, ToolChoice,
    ToolResultContent, Usage,
};

use crate::llm::accumulate::MessageAccumulator;
use crate::llm::router::Router;
use crate::llm::ChatProvider;

/// A stream of [`AgentEvent`]s from [`Agent::stream_turn`] /
/// [`Agent::resume_stream`]. Boxed for the same reason
/// `lib::llm::ChatStream` is: each call builds a distinct, non-nameable
/// `async_stream` type.
pub type AgentStream = Pin<Box<dyn Stream<Item = Result<AgentEvent, AgentError>> + Send>>;

pub struct Agent {
    provider: Arc<dyn ChatProvider>,
    spec: AgentSpec,
    tools: ToolBox,
}

impl Agent {
    pub fn builder(model: ModelRef, max_tokens: u32) -> AgentBuilder {
        AgentBuilder {
            spec: AgentSpec::new(model, max_tokens),
            tools: Vec::new(),
        }
    }

    /// The serializable definition of this agent — hand this to the frontend
    /// over IPC rather than anything execution-shaped.
    pub fn spec(&self) -> &AgentSpec {
        &self.spec
    }

    /// Append `text` as a user turn and run [`Agent::next_turn`].
    pub async fn send(
        &self,
        mut conversation: Conversation,
        text: impl Into<String>,
    ) -> Result<AgentTurn, AgentError> {
        conversation
            .messages
            .push(Message::user(vec![ContentBlock::Text {
                text: text.into(),
            }]));
        self.next_turn(conversation).await
    }

    /// Run the conversation forward until the model gives a final answer, a
    /// tool call needs approval, or `max_steps` is hit. See this module's doc
    /// and the "Loop semantics" section of the design this implements.
    pub async fn next_turn(&self, conversation: Conversation) -> Result<AgentTurn, AgentError> {
        self.run_loop(conversation, Vec::new(), Usage::default(), 0)
            .await
    }

    /// Continue a turn that stopped at [`TurnStop::AwaitingApproval`], having
    /// resolved every pending tool call with a decision. Errors if `turn`
    /// wasn't awaiting approval, if a pending call has no decision, if a
    /// decision names a call that wasn't pending, or if `turn.conversation`
    /// was modified since it was returned (see
    /// [`AgentError::ConversationMismatch`]).
    pub async fn resume(
        &self,
        turn: AgentTurn,
        decisions: &[ToolDecision],
    ) -> Result<AgentTurn, AgentError> {
        let AgentTurn {
            mut conversation,
            mut added,
            usage,
            steps,
            stop,
        } = turn;
        let TurnStop::AwaitingApproval { pending, completed } = stop else {
            return Err(AgentError::NotAwaitingApproval);
        };
        let (original_order, decision_map) =
            validate_and_index_decisions(&conversation, &pending, &completed, decisions)?;

        let mut by_id = index_by_tool_use_id(completed);
        for p in &pending {
            let decision = &decision_map[&p.tool_use_id];
            let result = match decision {
                Decision::Approve => {
                    self.execute_tool_call(&p.tool_use_id, &p.name, &p.input)
                        .await
                }
                Decision::Deny { reason } => deny_result(&p.tool_use_id, reason.clone()),
            };
            by_id.insert(p.tool_use_id.clone(), result);
        }

        let merged = merge_in_order(original_order, by_id);
        let user_message = Message::user(merged);
        conversation.messages.push(user_message.clone());
        added.push(user_message);

        self.run_loop(conversation, added, usage, steps).await
    }

    /// The streaming equivalent of [`Agent::next_turn`]. Returns the stream
    /// directly rather than `Result<AgentStream, _>` — a turn makes several
    /// provider calls, so there is no single "first" error to surface up
    /// front; the first failure, if any, is simply the first stream item.
    pub fn stream_turn(self: &Arc<Self>, conversation: Conversation) -> AgentStream {
        self.clone().run_stream(StreamStart::Fresh(conversation))
    }

    /// The streaming equivalent of [`Agent::resume`]. Validation that doesn't
    /// need a provider call (is this turn awaiting approval? does every
    /// pending call have exactly one decision? does the conversation still
    /// match?) happens up front, so an invalid call fails on the very first
    /// stream item rather than after doing any work.
    pub fn resume_stream(
        self: &Arc<Self>,
        turn: AgentTurn,
        decisions: Vec<ToolDecision>,
    ) -> AgentStream {
        let AgentTurn {
            conversation,
            added,
            usage,
            steps,
            stop,
        } = turn;
        let TurnStop::AwaitingApproval { pending, completed } = stop else {
            return err_stream(AgentError::NotAwaitingApproval);
        };
        let (original_order, decisions) =
            match validate_and_index_decisions(&conversation, &pending, &completed, &decisions) {
                Ok(v) => v,
                Err(err) => return err_stream(err),
            };
        self.clone().run_stream(StreamStart::Resumed {
            conversation,
            added,
            usage,
            steps,
            pending,
            completed,
            original_order,
            decisions,
        })
    }

    fn chat_options_for_step(&self, steps: u32) -> ChatOptions {
        let mut options = self.spec.chat_options();
        // A forced tool choice only makes sense on the model's first call in
        // a turn — otherwise the model could never end its own turn, and
        // every conversation would run to `max_steps`.
        if steps > 0
            && matches!(
                options.tool_choice,
                Some(ToolChoice::Any) | Some(ToolChoice::Tool { .. })
            )
        {
            options.tool_choice = Some(ToolChoice::Auto);
        }
        options
    }

    async fn execute_tool_call(
        &self,
        id: &str,
        name: &str,
        input: &serde_json::Value,
    ) -> ContentBlock {
        match self.tools.get(name) {
            Some(tool) => match tool.call(input.clone()).await {
                Ok(output) => ContentBlock::ToolResult {
                    tool_use_id: id.to_string(),
                    content: output.content,
                    is_error: false,
                },
                Err(err) => ContentBlock::ToolResult {
                    tool_use_id: id.to_string(),
                    content: vec![ToolResultContent::Text {
                        text: format!("tool {name:?} failed: {err:#}"),
                    }],
                    is_error: true,
                },
            },
            None => ContentBlock::ToolResult {
                tool_use_id: id.to_string(),
                content: vec![ToolResultContent::Text {
                    text: format!("unknown tool {name:?}"),
                }],
                is_error: true,
            },
        }
    }

    /// The blocking loop: steps 1-7 (and 9-10) of the design's "Loop
    /// semantics". `resume` and `next_turn` both funnel into this once
    /// they've established a starting `conversation`/`added`/`usage`/`steps`.
    async fn run_loop(
        &self,
        mut conversation: Conversation,
        mut added: Vec<Message>,
        mut usage: Usage,
        mut steps: u32,
    ) -> Result<AgentTurn, AgentError> {
        // The agent's spec is authoritative for the system prompt — whatever
        // `conversation.system` the caller passed in (commonly `None`, since
        // it's this agent's job to supply one) is replaced here, once, rather
        // than trusting each caller to keep it in sync with the spec.
        conversation.system = self.spec.system_prompt();
        loop {
            if steps >= self.spec.max_steps {
                return Ok(AgentTurn {
                    conversation,
                    added,
                    usage,
                    steps,
                    stop: TurnStop::MaxSteps,
                });
            }

            let options = self.chat_options_for_step(steps);
            let response = self.provider.complete(&conversation, &options).await?;
            steps += 1;
            usage = add_usage(usage, response.usage);

            let content = rewrite_tool_use_ids(response.content, steps);
            let assistant_message = Message::assistant(content.clone());
            conversation.messages.push(assistant_message.clone());
            added.push(assistant_message);

            let tool_uses = collect_tool_uses(&content);
            if tool_uses.is_empty() {
                if response.stop_reason == StopReason::ToolUse {
                    return Err(AgentError::UnhandledToolUse {
                        stop_reason: response.stop_reason,
                    });
                }
                return Ok(AgentTurn {
                    conversation,
                    added,
                    usage,
                    steps,
                    stop: TurnStop::Done {
                        stop_reason: response.stop_reason,
                    },
                });
            }

            let mut completed_results = Vec::new();
            let mut pending = Vec::new();
            for (id, name, input) in &tool_uses {
                match self
                    .tools
                    .get(name)
                    .map(|t| t.approval())
                    .unwrap_or(Approval::Automatic)
                {
                    Approval::Automatic => {
                        completed_results.push(self.execute_tool_call(id, name, input).await);
                    }
                    Approval::RequiresApproval => {
                        pending.push(ToolApprovalRequest {
                            tool_use_id: id.clone(),
                            name: name.clone(),
                            input: input.clone(),
                        });
                    }
                }
            }

            if !pending.is_empty() {
                return Ok(AgentTurn {
                    conversation,
                    added,
                    usage,
                    steps,
                    stop: TurnStop::AwaitingApproval {
                        pending,
                        completed: completed_results,
                    },
                });
            }

            let user_message = Message::user(completed_results);
            conversation.messages.push(user_message.clone());
            added.push(user_message);
        }
    }

    /// The streaming loop. Mirrors `run_loop` step for step, but drives each
    /// model call through `ChatProvider::stream` + a fresh
    /// `MessageAccumulator` (never one accumulator across the whole turn —
    /// see `shared::agent::AgentEvent::Model`'s doc for why), and yields an
    /// `AgentEvent` for everything `run_loop` does silently.
    fn run_stream(self: Arc<Self>, start: StreamStart) -> AgentStream {
        let this = self;
        let stream = async_stream::try_stream! {
            let (mut conversation, mut added, mut usage, mut steps) = match start {
                StreamStart::Fresh(conversation) => (conversation, Vec::new(), Usage::default(), 0),
                StreamStart::Resumed {
                    mut conversation,
                    mut added,
                    usage,
                    steps,
                    pending,
                    completed,
                    original_order,
                    decisions,
                } => {
                    let mut by_id = index_by_tool_use_id(completed);
                    for p in &pending {
                        let decision = &decisions[&p.tool_use_id];
                        yield AgentEvent::ToolStart {
                            tool_use_id: p.tool_use_id.clone(),
                            name: p.name.clone(),
                            input: p.input.clone(),
                        };
                        let result = match decision {
                            Decision::Approve => this.execute_tool_call(&p.tool_use_id, &p.name, &p.input).await,
                            Decision::Deny { reason } => deny_result(&p.tool_use_id, reason.clone()),
                        };
                        yield AgentEvent::ToolEnd {
                            tool_use_id: p.tool_use_id.clone(),
                            name: p.name.clone(),
                            result: result.clone(),
                        };
                        by_id.insert(p.tool_use_id.clone(), result);
                    }
                    let merged = merge_in_order(original_order, by_id);
                    let user_message = Message::user(merged);
                    conversation.messages.push(user_message.clone());
                    added.push(user_message);
                    (conversation, added, usage, steps)
                }
            };
            // See `run_loop`'s matching comment: the spec is authoritative.
            conversation.system = this.spec.system_prompt();

            loop {
                if steps >= this.spec.max_steps {
                    yield AgentEvent::Turn(AgentTurn { conversation, added, usage, steps, stop: TurnStop::MaxSteps });
                    return;
                }

                yield AgentEvent::StepStart { step: steps };
                let options = this.chat_options_for_step(steps);
                let mut provider_stream = this.provider.stream(&conversation, &options).await?;
                let mut acc = MessageAccumulator::new();
                while let Some(event) = provider_stream.next().await {
                    let event = event?;
                    acc.push(event.clone())?;
                    yield AgentEvent::Model { step: steps, event };
                }
                let response = acc.finish()?;
                steps += 1;
                usage = add_usage(usage, response.usage);

                let content = rewrite_tool_use_ids(response.content, steps);
                let assistant_message = Message::assistant(content.clone());
                conversation.messages.push(assistant_message.clone());
                added.push(assistant_message);

                let tool_uses = collect_tool_uses(&content);
                if tool_uses.is_empty() {
                    if response.stop_reason == StopReason::ToolUse {
                        // The `return` here (rather than relying on `?` alone) is
                        // load-bearing for the borrow checker, not just style: `?`'s
                        // static type doesn't prove non-fallthrough the way a real
                        // `return`/`panic!` does, so without it rustc considers
                        // `response.stop_reason` used-after-move below.
                        Err(AgentError::UnhandledToolUse { stop_reason: response.stop_reason })?;
                        return;
                    }
                    yield AgentEvent::Turn(AgentTurn {
                        conversation, added, usage, steps,
                        stop: TurnStop::Done { stop_reason: response.stop_reason },
                    });
                    return;
                }

                let mut completed_results = Vec::new();
                let mut pending = Vec::new();
                for (id, name, input) in &tool_uses {
                    match this.tools.get(name).map(|t| t.approval()).unwrap_or(Approval::Automatic) {
                        Approval::Automatic => {
                            yield AgentEvent::ToolStart { tool_use_id: id.clone(), name: name.clone(), input: input.clone() };
                            let result = this.execute_tool_call(id, name, input).await;
                            yield AgentEvent::ToolEnd { tool_use_id: id.clone(), name: name.clone(), result: result.clone() };
                            completed_results.push(result);
                        }
                        Approval::RequiresApproval => {
                            pending.push(ToolApprovalRequest { tool_use_id: id.clone(), name: name.clone(), input: input.clone() });
                        }
                    }
                }

                if !pending.is_empty() {
                    yield AgentEvent::Turn(AgentTurn {
                        conversation, added, usage, steps,
                        stop: TurnStop::AwaitingApproval { pending, completed: completed_results },
                    });
                    return;
                }

                let user_message = Message::user(completed_results);
                conversation.messages.push(user_message.clone());
                added.push(user_message);
            }
        };
        Box::pin(stream)
    }
}

enum StreamStart {
    Fresh(Conversation),
    Resumed {
        conversation: Conversation,
        added: Vec<Message>,
        usage: Usage,
        steps: u32,
        pending: Vec<ToolApprovalRequest>,
        completed: Vec<ContentBlock>,
        original_order: Vec<String>,
        decisions: HashMap<String, Decision>,
    },
}

fn err_stream(err: AgentError) -> AgentStream {
    Box::pin(futures_util::stream::once(async move { Err(err) }))
}

/// Rewrites every `ToolUse` block's id to be unique across the whole turn,
/// not just within one provider response. Ollama restarts its own id
/// counter every response (`ollama::wire::synth_tool_id`), so two steps of
/// one turn could otherwise both produce `"ollama-tool-0"` — harmless to the
/// wire layer (which only ever compares an id against itself within one
/// request build) but a correctness hazard for anything that resolves a
/// `tool_use_id` against the whole conversation, like an approval UI. The
/// new id is used everywhere downstream (`ToolApprovalRequest`, the
/// synthesized `ToolResult`); the id the provider actually sent is never
/// exposed outside this function.
fn rewrite_tool_use_ids(content: Vec<ContentBlock>, step: u32) -> Vec<ContentBlock> {
    let mut ordinal = 0u32;
    content
        .into_iter()
        .map(|block| match block {
            ContentBlock::ToolUse { name, input, .. } => {
                let id = format!("call_{step}_{ordinal}");
                ordinal += 1;
                ContentBlock::ToolUse { id, name, input }
            }
            other => other,
        })
        .collect()
}

/// `(tool_use_id, name, input)` for every `ToolUse` block, in original order.
fn collect_tool_uses(content: &[ContentBlock]) -> Vec<(String, String, serde_json::Value)> {
    content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::ToolUse { id, name, input } => {
                Some((id.clone(), name.clone(), input.clone()))
            }
            _ => None,
        })
        .collect()
}

fn add_usage(a: Usage, b: Usage) -> Usage {
    Usage {
        input_tokens: a.input_tokens + b.input_tokens,
        output_tokens: a.output_tokens + b.output_tokens,
        cache_read_input_tokens: add_optional(a.cache_read_input_tokens, b.cache_read_input_tokens),
        cache_creation_input_tokens: add_optional(
            a.cache_creation_input_tokens,
            b.cache_creation_input_tokens,
        ),
    }
}

fn add_optional(a: Option<u32>, b: Option<u32>) -> Option<u32> {
    match (a, b) {
        (None, None) => None,
        (a, b) => Some(a.unwrap_or(0) + b.unwrap_or(0)),
    }
}

fn deny_result(tool_use_id: &str, reason: Option<String>) -> ContentBlock {
    let text = reason.unwrap_or_else(|| "the user denied this tool call".to_string());
    ContentBlock::ToolResult {
        tool_use_id: tool_use_id.to_string(),
        content: vec![ToolResultContent::Text { text }],
        is_error: true,
    }
}

/// Every `ToolUse` id in the conversation's trailing assistant message, in
/// the order the model asked for them — the id order a merged set of
/// `tool_result`s must be sent back in.
fn last_assistant_tool_use_ids(conversation: &Conversation) -> Vec<String> {
    conversation
        .messages
        .last()
        .map(|message| {
            message
                .content
                .iter()
                .filter_map(|block| match block {
                    ContentBlock::ToolUse { id, .. } => Some(id.clone()),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

fn index_by_tool_use_id(results: Vec<ContentBlock>) -> HashMap<String, ContentBlock> {
    results
        .into_iter()
        .filter_map(|block| match &block {
            ContentBlock::ToolResult { tool_use_id, .. } => Some((tool_use_id.clone(), block)),
            _ => None,
        })
        .collect()
}

fn merge_in_order(
    order: Vec<String>,
    mut by_id: HashMap<String, ContentBlock>,
) -> Vec<ContentBlock> {
    order
        .into_iter()
        .filter_map(|id| by_id.remove(&id))
        .collect()
}

/// Checks that `turn.conversation` still matches the tool calls
/// `AwaitingApproval` recorded, and that `decisions` covers every pending
/// call exactly once. Returns the trailing assistant message's `ToolUse` id
/// order (the order the merged results must be sent back in) and a
/// `tool_use_id -> Decision` map on success.
fn validate_and_index_decisions(
    conversation: &Conversation,
    pending: &[ToolApprovalRequest],
    completed: &[ContentBlock],
    decisions: &[ToolDecision],
) -> Result<(Vec<String>, HashMap<String, Decision>), AgentError> {
    let original_order = last_assistant_tool_use_ids(conversation);

    let mut expected: BTreeSet<String> = pending.iter().map(|p| p.tool_use_id.clone()).collect();
    for block in completed {
        if let ContentBlock::ToolResult { tool_use_id, .. } = block {
            expected.insert(tool_use_id.clone());
        }
    }
    let actual: BTreeSet<String> = original_order.iter().cloned().collect();
    if actual != expected {
        return Err(AgentError::ConversationMismatch);
    }

    let mut decision_map = HashMap::new();
    for decision in decisions {
        if !pending
            .iter()
            .any(|p| p.tool_use_id == decision.tool_use_id)
        {
            return Err(AgentError::UnknownDecision {
                tool_use_id: decision.tool_use_id.clone(),
            });
        }
        decision_map.insert(decision.tool_use_id.clone(), decision.decision.clone());
    }
    for p in pending {
        if !decision_map.contains_key(&p.tool_use_id) {
            return Err(AgentError::MissingDecision {
                tool_use_id: p.tool_use_id.clone(),
            });
        }
    }

    Ok((original_order, decision_map))
}

/// Accumulates an agent's tools and per-request settings, then resolves a
/// concrete provider and produces an [`Agent`]. `model` and `max_tokens` are
/// positional and required — mirrors `shared::llm::ChatOptions::new`, which
/// establishes the same "both required, no sensible default" rule this
/// crate already follows for a model id and a token budget.
pub struct AgentBuilder {
    spec: AgentSpec,
    tools: Vec<Arc<dyn Tool>>,
}

impl AgentBuilder {
    /// Appends a piece of the system prompt — call this more than once to
    /// compose one from several pieces; see `AgentSpec::system_prompt`.
    pub fn system(mut self, text: impl Into<String>) -> Self {
        self.spec.system.push(text.into());
        self
    }

    pub fn tool(mut self, tool: impl Tool + 'static) -> Self {
        self.tools.push(Arc::new(tool));
        self
    }

    pub fn tools(mut self, tools: impl IntoIterator<Item = Arc<dyn Tool>>) -> Self {
        self.tools.extend(tools);
        self
    }

    pub fn tool_choice(mut self, choice: ToolChoice) -> Self {
        self.spec.tool_choice = Some(choice);
        self
    }

    pub fn thinking(mut self, thinking: Thinking) -> Self {
        self.spec.thinking = thinking;
        self
    }

    pub fn stop_sequence(mut self, sequence: impl Into<String>) -> Self {
        self.spec.stop_sequences.push(sequence.into());
        self
    }

    /// Cap on model calls in one turn. See `AgentSpec.max_steps`'s doc for
    /// why this — not a per-turn token budget — is the primary
    /// cost/runaway control.
    pub fn max_steps(mut self, max_steps: u32) -> Self {
        self.spec.max_steps = max_steps;
        self
    }

    /// Resolve `model` through `router` and build the agent.
    pub fn build(self, router: &Router) -> Result<Agent, AgentError> {
        let provider = router.chat(&self.spec.model)?;
        self.build_with(provider)
    }

    /// Build directly from a concrete provider, skipping the router — what
    /// this crate's own tests use, and useful for a caller that already has
    /// one client and doesn't want to stand up a whole `Router` for it.
    pub fn build_with(mut self, provider: Arc<dyn ChatProvider>) -> Result<Agent, AgentError> {
        let mut tool_box = ToolBox::new();
        for tool in self.tools.drain(..) {
            tool_box.add_arc(tool)?;
        }
        self.spec.tools = tool_box.specs();
        Ok(Agent {
            provider,
            spec: self.spec,
            tools: tool_box,
        })
    }
}

#[cfg(test)]
mod tests;
