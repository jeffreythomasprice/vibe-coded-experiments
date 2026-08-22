//! Integration tests that actually call the Anthropic API — and spend real
//! money doing it. A plain `cargo test` compiles this file but every test
//! below returns immediately unless `AI_HARNESS_LIVE=1` is set, and each one
//! also skips (rather than fails) if `ANTHROPIC_API_KEY` isn't set, so a
//! misconfigured run reads as "skipped", not as a broken build.
//!
//!     AI_HARNESS_LIVE=1 cargo test -p lib --test live_anthropic -- --nocapture

mod common;

use futures_util::StreamExt;

use lib::llm::accumulate::MessageAccumulator;
use lib::llm::config::AnthropicConfig;
use lib::llm::{AnthropicClient, ChatProvider, Freshness, ModelProvider};
use shared::llm::{
    ChatOptions, ContentBlock, Conversation, Effort, Message, StopReason, Thinking, ToolDef,
};

/// The model this file's tests call. A one-line edit point for a cheaper
/// smoke run — see `crate::llm::config`'s module doc for why this lives here
/// as a constant rather than as a config default.
const MODEL: &str = "claude-opus-5";

/// Build the client, or print why we're skipping and return `None`.
fn client(test_name: &str) -> Option<AnthropicClient> {
    if common::skip_unless_live(test_name) {
        return None;
    }
    let cfg = AnthropicConfig::default();
    if let Err(err) = cfg.api_key() {
        eprintln!("skipping {test_name}: {err}");
        return None;
    }
    Some(AnthropicClient::new(cfg, common::TIMEOUT, 2).expect("client construction"))
}

/// Low effort and a modest `max_tokens`: this is a smoke test, not a
/// reasoning benchmark, and thinking is on by default on current models —
/// low effort keeps the bill (and the odds of hitting `max_tokens` before
/// any visible text) small.
fn cheap_options() -> ChatOptions {
    ChatOptions {
        thinking: Thinking::Adaptive { effort: Effort::Low },
        ..ChatOptions::new(MODEL, 1024)
    }
}

#[tokio::test]
async fn completes_a_turn() {
    let Some(client) = client("completes_a_turn") else {
        return;
    };
    let conversation = Conversation {
        system: None,
        messages: vec![Message::user(vec![ContentBlock::Text {
            text: "Reply with exactly one word: pong".to_string(),
        }])],
    };
    let message = client
        .complete(&conversation, &cheap_options())
        .await
        .expect("complete() failed");

    assert_eq!(message.stop_reason, StopReason::EndTurn);
    assert!(
        message.content.iter().any(|b| matches!(b, ContentBlock::Text { .. })),
        "expected a text block in {:?}",
        message.content
    );
}

#[tokio::test]
async fn streams_deltas_that_accumulate_to_the_same_message() {
    let Some(client) = client("streams_deltas_that_accumulate_to_the_same_message") else {
        return;
    };
    let conversation = Conversation {
        system: None,
        messages: vec![Message::user(vec![ContentBlock::Text {
            text: "Reply with exactly one word: pong".to_string(),
        }])],
    };
    let mut stream = client
        .stream(&conversation, &cheap_options())
        .await
        .expect("stream() failed");

    let mut accumulator = MessageAccumulator::new();
    while let Some(event) = stream.next().await {
        let event = event.expect("stream yielded an error");
        accumulator.push(event).expect("event did not fit the accumulator");
    }
    let message = accumulator.finish().expect("stream never sent message_stop");

    assert_eq!(message.stop_reason, StopReason::EndTurn);
    assert!(!message.content.is_empty());
}

#[tokio::test]
async fn calls_a_tool() {
    let Some(client) = client("calls_a_tool") else {
        return;
    };
    let tool = ToolDef {
        name: "get_current_time".to_string(),
        description: "Returns the current time. Call this whenever the user asks what time it is."
            .to_string(),
        input_schema: serde_json::json!({"type": "object", "properties": {}, "required": []}),
    };
    let conversation = Conversation {
        system: None,
        messages: vec![Message::user(vec![ContentBlock::Text {
            text: "What time is it right now? Use the tool to find out.".to_string(),
        }])],
    };
    let options = ChatOptions {
        tools: vec![tool],
        ..cheap_options()
    };
    let message = client
        .complete(&conversation, &options)
        .await
        .expect("complete() failed");

    assert_eq!(message.stop_reason, StopReason::ToolUse);
    let tool_use = message
        .content
        .iter()
        .find(|b| matches!(b, ContentBlock::ToolUse { .. }));
    assert!(tool_use.is_some(), "expected a tool_use block in {:?}", message.content);
    if let Some(ContentBlock::ToolUse { name, .. }) = tool_use {
        assert_eq!(name, "get_current_time");
    }
}

#[tokio::test]
async fn lists_models() {
    let Some(client) = client("lists_models") else {
        return;
    };
    let models = client
        .list_models(Freshness::Refresh)
        .await
        .expect("list_models failed");

    assert!(!models.is_empty(), "expected at least one model");
    assert!(
        models.iter().all(|m| m.id.starts_with("claude")),
        "expected every id to start with \"claude\", got {:?}",
        models.iter().map(|m| &m.id).collect::<Vec<_>>()
    );
}
