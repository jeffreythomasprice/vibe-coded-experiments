//! Integration tests that actually call the OpenAI API — and spend real
//! money doing it, including generating an image. A plain `cargo test`
//! compiles this file but every test below returns immediately unless
//! `AI_HARNESS_LIVE=1` is set, and each one also skips (rather than fails)
//! if `OPENAI_API_KEY` isn't set.
//!
//!     AI_HARNESS_LIVE=1 cargo test -p lib --test live_openai -- --nocapture

mod common;

use futures_util::StreamExt;

use lib::llm::accumulate::MessageAccumulator;
use lib::llm::{ChatProvider, ImageProvider, OpenAiClient};
use shared::llm::{ChatOptions, ContentBlock, Conversation, ImageRequest, Message, ToolDef};

fn client(test_name: &str) -> Option<OpenAiClient> {
    if common::skip_unless_live(test_name) {
        return None;
    }
    let cfg = common::openai_config();
    if let Err(err) = cfg.api_key() {
        eprintln!("skipping {test_name}: {err}");
        return None;
    }
    Some(OpenAiClient::new(cfg, common::TIMEOUT, 2).expect("client construction"))
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
    let options = ChatOptions {
        max_tokens: Some(256),
        ..ChatOptions::default()
    };
    let message = client
        .complete(&conversation, &options)
        .await
        .expect("complete() failed");

    assert!(
        message.content.iter().any(|b| matches!(b, ContentBlock::Text { .. })),
        "expected a text block in {:?}",
        message.content
    );
}

#[tokio::test]
async fn streams_deltas() {
    let Some(client) = client("streams_deltas") else {
        return;
    };
    let conversation = Conversation {
        system: None,
        messages: vec![Message::user(vec![ContentBlock::Text {
            text: "Reply with exactly one word: pong".to_string(),
        }])],
    };
    let options = ChatOptions {
        max_tokens: Some(256),
        ..ChatOptions::default()
    };
    let mut stream = client
        .stream(&conversation, &options)
        .await
        .expect("stream() failed");

    let mut accumulator = MessageAccumulator::new();
    while let Some(event) = stream.next().await {
        let event = event.expect("stream yielded an error");
        accumulator.push(event).expect("event did not fit the accumulator");
    }
    let message = accumulator.finish().expect("stream never sent message_stop");
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
        max_tokens: Some(256),
        tools: vec![tool],
        ..ChatOptions::default()
    };
    let message = client
        .complete(&conversation, &options)
        .await
        .expect("complete() failed");

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
async fn generates_an_image() {
    let Some(client) = client("generates_an_image") else {
        return;
    };
    let request = ImageRequest {
        prompt: "a single red circle on a plain white background".to_string(),
        model: None,
        size: Some("1024x1024".to_string()),
        quality: Some("low".to_string()),
        background: None,
        output_format: Some("png".to_string()),
        n: 1,
    };
    let images = client.generate(&request).await.expect("generate() failed");
    assert_eq!(images.len(), 1);

    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&images[0].data)
        .expect("b64_json did not decode as base64");
    assert!(
        bytes.starts_with(&[0x89, b'P', b'N', b'G']),
        "decoded image did not start with the PNG magic bytes"
    );

    let path = std::env::temp_dir().join("ai-harness-live-openai-generated.png");
    std::fs::write(&path, &bytes).expect("failed to write the generated image to disk");
    eprintln!("wrote generated image to {}", path.display());
}
