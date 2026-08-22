//! An in-process `ChatProvider` test double, shared by `router`'s and
//! `crate::agent`'s unit tests.
//!
//! This repo has zero mocking dependencies and `CLAUDE.md` asks that unit
//! tests stay that way — see the module docs on each provider's `wire.rs` for
//! the fixture-based pattern this deliberately doesn't try to be. What's
//! under test at the call sites of [`ScriptedProvider`] is the *agent loop*
//! and the *router*, not any provider's wire format — that already has its
//! own fixture tests per provider. So this needs no wire format at all: it
//! replays a scripted list of responses and records what it was asked.

#![allow(dead_code)]

use std::collections::VecDeque;
use std::sync::Mutex;

use async_trait::async_trait;
use futures_util::stream;
use shared::llm::{ChatOptions, CompletedMessage, Conversation, StreamEvent};

use super::{ChatProvider, ChatStream};
use crate::llm::error::LlmError;

/// Replays a scripted list of responses, in order, across both `complete`
/// and `stream` — the two draw from the same queue, so a test mixing
/// blocking and streaming calls still sees them consumed in call order.
pub(crate) struct ScriptedProvider {
    name: &'static str,
    responses: Mutex<VecDeque<Result<CompletedMessage, LlmError>>>,
    pub(crate) calls: Mutex<Vec<(Conversation, ChatOptions)>>,
}

impl ScriptedProvider {
    pub(crate) fn new(responses: Vec<Result<CompletedMessage, LlmError>>) -> Self {
        Self {
            name: "scripted",
            responses: Mutex::new(responses.into()),
            calls: Mutex::new(Vec::new()),
        }
    }

    /// Override the provider name reported by `ChatProvider::name` — for
    /// router tests that want to see it show up in an error message.
    pub(crate) fn named(mut self, name: &'static str) -> Self {
        self.name = name;
        self
    }

    pub(crate) fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
}

#[async_trait]
impl ChatProvider for ScriptedProvider {
    fn name(&self) -> &'static str {
        self.name
    }

    async fn complete(
        &self,
        conversation: &Conversation,
        options: &ChatOptions,
    ) -> Result<CompletedMessage, LlmError> {
        self.calls
            .lock()
            .unwrap()
            .push((conversation.clone(), options.clone()));
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| panic!("ScriptedProvider ran out of scripted responses"))
    }

    /// Synthesizes a stream from the same script `complete` would have
    /// served, shaped so folding it with
    /// [`crate::llm::accumulate::MessageAccumulator`] reproduces the exact
    /// `CompletedMessage` `complete` would have returned for the same call —
    /// see [`synthesize_stream_events`].
    async fn stream(
        &self,
        conversation: &Conversation,
        options: &ChatOptions,
    ) -> Result<ChatStream, LlmError> {
        let message = self.complete(conversation, options).await?;
        let events = synthesize_stream_events(&message);
        let stream = stream::iter(events.into_iter().map(Ok::<_, LlmError>));
        Ok(Box::pin(stream))
    }
}

/// Turn a `CompletedMessage` into a `StreamEvent` sequence that accumulates
/// back to itself exactly. Each block arrives whole in its own `BlockStart`
/// (a valid, if maximally boring, stream — `MessageAccumulator::push` never
/// requires an empty shell followed by deltas, only the doc comment on
/// `StreamEvent::BlockStart` describes that as the common case), so there is
/// no delta-fragmentation logic here to get subtly wrong.
pub(crate) fn synthesize_stream_events(message: &CompletedMessage) -> Vec<StreamEvent> {
    let mut events = Vec::new();
    if let Some(model) = &message.model {
        events.push(StreamEvent::MessageStart {
            model: model.clone(),
        });
    }
    for (index, block) in message.content.iter().enumerate() {
        events.push(StreamEvent::BlockStart {
            index,
            block: block.clone(),
        });
        events.push(StreamEvent::BlockStop { index });
    }
    events.push(StreamEvent::MessageStop {
        stop_reason: message.stop_reason.clone(),
        usage: message.usage,
    });
    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::accumulate::MessageAccumulator;
    use shared::llm::{ContentBlock, Role, StopReason, Usage};

    fn sample_message() -> CompletedMessage {
        CompletedMessage {
            role: Role::Assistant,
            content: vec![
                ContentBlock::Text {
                    text: "hello".to_string(),
                },
                ContentBlock::ToolUse {
                    id: "call_0".to_string(),
                    name: "get_weather".to_string(),
                    input: serde_json::json!({"city": "Paris"}),
                },
            ],
            stop_reason: StopReason::ToolUse,
            usage: Usage {
                input_tokens: 12,
                output_tokens: 7,
                ..Default::default()
            },
            model: Some("claude-opus-5".to_string()),
        }
    }

    #[tokio::test]
    async fn complete_returns_scripted_responses_in_order_and_records_calls() {
        let provider = ScriptedProvider::new(vec![
            Ok(sample_message()),
            Err(LlmError::MissingApiKey {
                var: "X".to_string(),
            }),
        ]);
        let conversation = Conversation::default();
        let options = ChatOptions::new("claude-opus-5", 1024);

        let first = provider.complete(&conversation, &options).await.unwrap();
        assert_eq!(first.content.len(), 2);
        let second = provider.complete(&conversation, &options).await;
        assert!(second.is_err());

        assert_eq!(provider.call_count(), 2);
    }

    #[test]
    #[should_panic(expected = "ran out of scripted responses")]
    fn complete_panics_loudly_when_the_script_runs_dry() {
        let provider = ScriptedProvider::new(vec![]);
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(provider.complete(&Conversation::default(), &ChatOptions::new("m", 1)))
            .ok();
    }

    #[tokio::test]
    async fn synthesized_stream_accumulates_back_to_the_scripted_message() {
        let message = sample_message();
        let provider = ScriptedProvider::new(vec![Ok(message.clone())]);
        let mut stream = provider
            .stream(
                &Conversation::default(),
                &ChatOptions::new("claude-opus-5", 1024),
            )
            .await
            .unwrap();

        let mut acc = MessageAccumulator::new();
        use futures_util::StreamExt;
        while let Some(event) = stream.next().await {
            acc.push(event.unwrap()).unwrap();
        }
        let accumulated = acc.finish().unwrap();

        assert_eq!(
            serde_json::to_value(&accumulated.content).unwrap(),
            serde_json::to_value(&message.content).unwrap()
        );
        assert_eq!(accumulated.stop_reason, message.stop_reason);
        assert_eq!(accumulated.usage, message.usage);
        assert_eq!(accumulated.model, message.model);
    }
}
