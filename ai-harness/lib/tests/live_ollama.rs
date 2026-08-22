//! Integration tests that actually call a local Ollama instance — free, but
//! needs `ollama serve` running with the right models pulled. A plain
//! `cargo test` compiles this file but every test below returns immediately
//! unless `AI_HARNESS_LIVE=1` is set, and each one also skips (rather than
//! fails) if Ollama isn't reachable, so a "forgot to start Ollama" run reads
//! as "skipped", not as a broken build.
//!
//!     ollama serve &
//!     ollama pull llama3.1:8b && ollama pull qwen3.5:latest
//!     AI_HARNESS_LIVE=1 cargo test -p lib --test live_ollama -- --nocapture

mod common;

use futures_util::StreamExt;

use lib::llm::accumulate::MessageAccumulator;
use lib::llm::config::OllamaConfig;
use lib::llm::{ChatProvider, LlmError, OllamaClient};
use shared::llm::{
    ChatOptions, ContentBlock, Conversation, ImageSource, MediaType, Message, StopReason, ToolDef,
};

fn client(test_name: &str, cfg: OllamaConfig) -> Option<OllamaClient> {
    if common::skip_unless_live(test_name) {
        return None;
    }
    Some(OllamaClient::new(cfg, common::TIMEOUT, 0).expect("client construction"))
}

/// Treat "Ollama isn't running" as a skip, not a failure — this test suite
/// shouldn't turn red just because the local server was never started.
fn skip_if_unreachable<T>(test_name: &str, result: Result<T, LlmError>) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(err) => {
            eprintln!("skipping {test_name}: could not reach Ollama: {err}");
            None
        }
    }
}

#[tokio::test]
async fn completes_a_turn() {
    let Some(client) = client("completes_a_turn", common::ollama_config()) else {
        return;
    };
    let conversation = Conversation {
        system: None,
        messages: vec![Message::user(vec![ContentBlock::Text {
            text: "Reply with exactly one word: pong".to_string(),
        }])],
    };
    let options = ChatOptions {
        max_tokens: Some(64),
        ..ChatOptions::default()
    };
    let Some(message) = skip_if_unreachable(
        "completes_a_turn",
        client.complete(&conversation, &options).await,
    ) else {
        return;
    };

    assert!(
        message.content.iter().any(|b| matches!(b, ContentBlock::Text { .. })),
        "expected a text block in {:?}",
        message.content
    );
}

#[tokio::test]
async fn streams_ndjson() {
    let Some(client) = client("streams_ndjson", common::ollama_config()) else {
        return;
    };
    let conversation = Conversation {
        system: None,
        messages: vec![Message::user(vec![ContentBlock::Text {
            text: "Reply with exactly one word: pong".to_string(),
        }])],
    };
    let options = ChatOptions {
        max_tokens: Some(64),
        ..ChatOptions::default()
    };
    let stream_result = client.stream(&conversation, &options).await;
    let Some(mut stream) = skip_if_unreachable("streams_ndjson", stream_result) else {
        return;
    };

    let mut accumulator = MessageAccumulator::new();
    while let Some(event) = stream.next().await {
        let event = event.expect("stream yielded an error");
        accumulator.push(event).expect("event did not fit the accumulator");
    }
    let message = accumulator.finish().expect("stream never sent message_stop");
    assert!(!message.content.is_empty());
}

#[tokio::test]
async fn describes_an_image() {
    // Vision needs a multimodal model — override the default regardless of
    // AI_HARNESS_LIVE_OLLAMA_MODEL, since a caller pointing that at a
    // text-only model for the other tests shouldn't break this one.
    let mut cfg = common::ollama_config();
    cfg.model = "qwen3.5:latest".to_string();
    let Some(client) = client("describes_an_image", cfg) else {
        return;
    };
    let conversation = Conversation {
        system: None,
        messages: vec![Message::user(vec![
            ContentBlock::Text {
                text: "What color is this image? Answer in one word.".to_string(),
            },
            ContentBlock::Image {
                source: ImageSource {
                    media_type: MediaType::Png,
                    data: common::pixel_png_base64(),
                },
            },
        ])],
    };
    let options = ChatOptions {
        max_tokens: Some(64),
        ..ChatOptions::default()
    };
    let Some(message) = skip_if_unreachable(
        "describes_an_image",
        client.complete(&conversation, &options).await,
    ) else {
        return;
    };

    // Assert on shape, not on wording — a vision model's exact phrasing
    // isn't something to pin a test to.
    assert!(
        message.content.iter().any(|b| matches!(b, ContentBlock::Text { .. })),
        "expected a text block describing the image in {:?}",
        message.content
    );
}

#[tokio::test]
async fn calls_a_tool() {
    let mut cfg = common::ollama_config();
    cfg.model = "llama3.1:8b".to_string();
    let Some(client) = client("calls_a_tool", cfg) else {
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
        max_tokens: Some(256),
        tools: vec![tool],
        ..ChatOptions::default()
    };
    let Some(message) = skip_if_unreachable("calls_a_tool", client.complete(&conversation, &options).await)
    else {
        return;
    };

    // Small local models sometimes leak a tool call as plain text instead of
    // the structured field (see `ollama::wire::recover_leaked_tool_call`) —
    // accept either recognized shape rather than demanding one specific
    // stop_reason, matching this crate's "never a panic" stance on that
    // failure mode.
    let called_the_tool = message.stop_reason == StopReason::ToolUse
        && message
            .content
            .iter()
            .any(|b| matches!(b, ContentBlock::ToolUse { .. }));
    assert!(
        called_the_tool,
        "expected a tool_use block in {:?}",
        message.content
    );
}
