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
use lib::llm::config::OpenAiConfig;
use lib::llm::{
    ChatProvider, EmbeddingProvider, Freshness, ImageProvider, LlmError, ModelProvider,
    OpenAiClient,
};
use shared::llm::{
    ChatOptions, ContentBlock, Conversation, EmbeddingRequest, ImageRequest, Message, ToolDef,
};

/// The models this file's tests call. One-line edit points for a cheaper
/// smoke run — see `crate::llm::config`'s module doc for why these live here
/// as constants rather than as config defaults.
const CHAT_MODEL: &str = "gpt-5.5";
const IMAGE_MODEL: &str = "gpt-image-2";
const EMBEDDING_MODEL: &str = "text-embedding-3-small";

fn client(test_name: &str) -> Option<OpenAiClient> {
    if common::skip_unless_live(test_name) {
        return None;
    }
    let cfg = OpenAiConfig::default();
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
    let options = ChatOptions::new(CHAT_MODEL, 256);
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
    let options = ChatOptions::new(CHAT_MODEL, 256);
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
        tools: vec![tool],
        ..ChatOptions::new(CHAT_MODEL, 256)
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
        size: Some("1024x1024".to_string()),
        quality: Some("low".to_string()),
        output_format: Some("png".to_string()),
        ..ImageRequest::new(IMAGE_MODEL, "a single red circle on a plain white background")
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
        models.iter().any(|m| m.id == CHAT_MODEL),
        "expected {CHAT_MODEL:?} to appear in {:?}",
        models.iter().map(|m| &m.id).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn lists_embedding_models() {
    let Some(client) = client("lists_embedding_models") else {
        return;
    };
    let models = client
        .list_embedding_models(Freshness::Refresh)
        .await
        .expect("list_embedding_models failed");

    assert!(!models.is_empty(), "expected at least one embedding model");
    for model in &models {
        assert!(
            model.details.is_embedding(),
            "list_embedding_models returned a non-embedding model: {model:?}"
        );
    }
    assert!(
        models.iter().any(|m| m.id == EMBEDDING_MODEL),
        "expected {EMBEDDING_MODEL:?} to appear in {:?}",
        models.iter().map(|m| &m.id).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn embeds_text() {
    let Some(client) = client("embeds_text") else {
        return;
    };
    let request = EmbeddingRequest::new(
        EMBEDDING_MODEL,
        vec!["the quick brown fox".to_string(), "a red bicycle".to_string()],
    );
    let response = client.embed(&request).await.expect("embed() failed");

    assert_eq!(response.embeddings.len(), 2);
    assert_eq!(response.embeddings[0].index, 0);
    assert_eq!(response.embeddings[1].index, 1);
    assert!(!response.embeddings[0].vector.is_empty());
    assert_eq!(
        response.embeddings[0].vector.len(),
        response.embeddings[1].vector.len(),
        "two inputs to the same model must produce equal-length vectors"
    );
    // Shape, not wording — two different inputs should not embed to the
    // exact same vector.
    assert_ne!(response.embeddings[0].vector, response.embeddings[1].vector);
}

#[tokio::test]
async fn rejects_oversized_input() {
    let Some(client) = client("rejects_oversized_input") else {
        return;
    };
    // Comfortably past every current OpenAI embedding model's 8192-token
    // limit — repetition rather than length keeps this a fast, cheap call.
    let huge_input = "the quick brown fox jumps over the lazy dog ".repeat(4000);
    let request = EmbeddingRequest::new(EMBEDDING_MODEL, vec![huge_input]);

    let err = client
        .embed(&request)
        .await
        .expect_err("expected embed() to reject oversized input");
    match &err {
        LlmError::InputTooLarge { message, .. } => {
            // Printed so a real rejection body can be captured and committed
            // as `openai/fixtures/error_context_length.json` if OpenAI's
            // wording has since changed from what that fixture assumes.
            eprintln!("live too-large rejection message: {message}");
        }
        other => panic!("expected InputTooLarge, got {other:?}"),
    }
}
