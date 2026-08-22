//! Integration tests that actually call a local Ollama instance — free, but
//! needs `ollama serve` running with the right models pulled. A plain
//! `cargo test` compiles this file but every test below returns immediately
//! unless `AI_HARNESS_LIVE=1` is set, and each one also skips (rather than
//! fails) if Ollama isn't reachable, so a "forgot to start Ollama" run reads
//! as "skipped", not as a broken build.
//!
//!     ollama serve &
//!     ollama pull llama3.1:8b && ollama pull qwen3.5:latest && ollama pull qwen3-embedding:8b
//!     AI_HARNESS_LIVE=1 cargo test -p lib --test live_ollama -- --nocapture

mod common;

use futures_util::StreamExt;

use lib::cache::Cache;
use lib::config::CacheConfig;
use lib::llm::accumulate::MessageAccumulator;
use lib::llm::config::OllamaConfig;
use lib::llm::{ChatProvider, EmbeddingProvider, Freshness, LlmError, ModelProvider, OllamaClient};
use shared::llm::{
    ChatOptions, ContentBlock, Conversation, EmbeddingRequest, ImageSource, MediaType, Message,
    StopReason, ToolDef,
};

/// The models this file's tests call. One-line edit points for a different
/// local pull — see `crate::llm::config`'s module doc for why these live
/// here as constants rather than as config defaults.
const CHAT_MODEL: &str = "qwen3.5:latest";
const VISION_MODEL: &str = "qwen3.5:latest";
const TOOL_MODEL: &str = "llama3.1:8b";
const EMBEDDING_MODEL: &str = "qwen3-embedding:8b";
const LIBRARY_MODEL: &str = "llama3.1";

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
    let Some(client) = client("completes_a_turn", OllamaConfig::default()) else {
        return;
    };
    let conversation = Conversation {
        system: None,
        messages: vec![Message::user(vec![ContentBlock::Text {
            text: "Reply with exactly one word: pong".to_string(),
        }])],
    };
    let options = ChatOptions::new(CHAT_MODEL, 64);
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
    let Some(client) = client("streams_ndjson", OllamaConfig::default()) else {
        return;
    };
    let conversation = Conversation {
        system: None,
        messages: vec![Message::user(vec![ContentBlock::Text {
            text: "Reply with exactly one word: pong".to_string(),
        }])],
    };
    let options = ChatOptions::new(CHAT_MODEL, 64);
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
    let Some(client) = client("describes_an_image", OllamaConfig::default()) else {
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
    let options = ChatOptions::new(VISION_MODEL, 64);
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
    let Some(client) = client("calls_a_tool", OllamaConfig::default()) else {
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
        ..ChatOptions::new(TOOL_MODEL, 256)
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

#[tokio::test]
async fn lists_local_models() {
    let Some(client) = client("lists_local_models", OllamaConfig::default()) else {
        return;
    };
    let Some(models) = skip_if_unreachable(
        "lists_local_models",
        client.list_models(Freshness::Refresh).await,
    ) else {
        return;
    };

    assert!(
        models.iter().any(|m| m.id == CHAT_MODEL),
        "expected {CHAT_MODEL:?} among the locally pulled models {:?}",
        models.iter().map(|m| &m.id).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn lists_embedding_models() {
    let Some(client) = client("lists_embedding_models", OllamaConfig::default()) else {
        return;
    };
    let Some(models) = skip_if_unreachable(
        "lists_embedding_models",
        client.list_embedding_models(Freshness::Refresh).await,
    ) else {
        return;
    };

    for model in &models {
        assert!(
            model.details.is_embedding(),
            "list_embedding_models returned a non-embedding model: {model:?}"
        );
    }
    assert!(
        models.iter().any(|m| m.id == EMBEDDING_MODEL),
        "expected {EMBEDDING_MODEL:?} among the locally pulled embedding models {:?} — \
         did you `ollama pull {EMBEDDING_MODEL}`?",
        models.iter().map(|m| &m.id).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn embeds_text() {
    let Some(client) = client("embeds_text", OllamaConfig::default()) else {
        return;
    };
    let request = EmbeddingRequest::new(
        EMBEDDING_MODEL,
        vec!["the quick brown fox".to_string(), "a red bicycle".to_string()],
    );
    let Some(response) = skip_if_unreachable("embeds_text", client.embed(&request).await) else {
        return;
    };

    assert_eq!(response.embeddings.len(), 2);
    assert_eq!(response.embeddings[0].index, 0);
    assert_eq!(response.embeddings[1].index, 1);
    assert!(!response.embeddings[0].vector.is_empty());
    assert_eq!(
        response.embeddings[0].vector.len(),
        response.embeddings[1].vector.len(),
        "two inputs to the same model must produce equal-length vectors"
    );
    assert_ne!(response.embeddings[0].vector, response.embeddings[1].vector);
}

#[tokio::test]
async fn rejects_oversized_input() {
    let Some(client) = client("rejects_oversized_input", OllamaConfig::default()) else {
        return;
    };
    // Repetition rather than length keeps this a fast, local-only call.
    // Sized well past even a large-context embedding model (confirmed live
    // against qwen3-embedding:8b's 40960-token window — nomic-embed-text's
    // is a much smaller 2048) so this test doesn't silently pass just
    // because EMBEDDING_MODEL above got pointed at a bigger-context model
    // than whatever was pulled when this was written.
    let huge_input = "the quick brown fox jumps over the lazy dog ".repeat(20_000);
    let request = EmbeddingRequest::new(EMBEDDING_MODEL, vec![huge_input]);

    let result = client.embed(&request).await;
    if matches!(result, Err(LlmError::Http { .. })) {
        // Only a connection-level failure counts as "Ollama isn't
        // reachable" — anything else, including a plain success, is this
        // test's own outcome to assert on below.
        skip_if_unreachable::<()>("rejects_oversized_input", result.map(|_| ()));
        return;
    }

    match result {
        Ok(_) => panic!(
            "expected embed() to reject oversized input — did truncate:false stop \
             being honored, or does this Ollama version not enforce the limit?"
        ),
        Err(LlmError::InputTooLarge { message, .. }) => {
            // Printed so a real rejection body can be captured and committed
            // as `ollama/fixtures/error_input_too_long.json` if the local
            // Ollama version's wording differs from what that fixture
            // assumes.
            eprintln!("live too-large rejection message: {message}");
        }
        Err(other) => panic!("expected InputTooLarge, got {other:?}"),
    }
}

/// Hits `ollama.com/library`, not the local server — free (no API key), but
/// still an internet call, so it stays behind `AI_HARNESS_LIVE` like every
/// other test in this file.
#[tokio::test]
async fn lists_remote_library_models() {
    let Some(client) = client("lists_remote_library_models", OllamaConfig::default()) else {
        return;
    };
    let models = client
        .list_models_remote(Freshness::Refresh)
        .await
        .expect("list_models_remote failed");

    assert!(
        models.len() > 100,
        "expected well over 100 library models, got {}",
        models.len()
    );
    assert!(
        models.iter().any(|m| m.id == LIBRARY_MODEL),
        "expected {LIBRARY_MODEL:?} in the library index"
    );
}

#[tokio::test]
async fn lists_remote_tags_for_a_model() {
    let Some(client) = client("lists_remote_tags_for_a_model", OllamaConfig::default()) else {
        return;
    };
    let tags = client
        .list_remote_tags(LIBRARY_MODEL, Freshness::Refresh)
        .await
        .expect("list_remote_tags failed");

    assert!(!tags.is_empty(), "expected at least one tag");
    let prefix = format!("{LIBRARY_MODEL}:");
    assert!(
        tags.iter().all(|m| m.id.starts_with(&prefix)),
        "expected every tag id to start with {prefix:?}, got {:?}",
        tags.iter().map(|m| &m.id).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn caches_the_remote_library_listing_on_disk() {
    if common::skip_unless_live("caches_the_remote_library_listing_on_disk") {
        return;
    }

    let dir = std::env::temp_dir().join(format!(
        "ai-harness-live-ollama-cache-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let cache = Cache::new(&CacheConfig {
        dir: dir.clone(),
        ttl_secs: 3600,
    });
    let cached_client = OllamaClient::new(OllamaConfig::default(), common::TIMEOUT, 0)
        .expect("client construction")
        .with_cache(cache);

    // Refresh writes to the cache; Cached must then read the same list back
    // without erroring, and the cache directory must hold a file for it.
    let refreshed = cached_client
        .list_models_remote(Freshness::Refresh)
        .await
        .expect("list_models_remote(Refresh) failed");
    let cached = cached_client
        .list_models_remote(Freshness::Cached)
        .await
        .expect("cached list_models_remote failed");

    assert_eq!(refreshed.len(), cached.len());
    assert!(
        dir.join("ollama-library.html").exists(),
        "expected a cache file to have been written to {}",
        dir.display()
    );

    let _ = std::fs::remove_dir_all(&dir);
}
