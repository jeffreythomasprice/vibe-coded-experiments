//! OpenAI Embeddings API (`POST {base_url}/v1/embeddings`) request/response
//! shapes and pure conversions to/from `shared::llm` types.
//!
//! Structured like `openai::images` — a second endpoint on the same client,
//! its own pure request/response/error functions, unit-tested on fixtures
//! under `fixtures/`. See `openai::mod` for the thin transport layer.

use serde::Deserialize;

use shared::llm::{Embedding, EmbeddingRequest, EmbeddingResponse, EmbeddingUsage};

use crate::llm::config::OpenAiConfig;
use crate::llm::error::LlmError;

use super::wire::{self, PROVIDER};

/// Build the JSON body for `POST {base_url}/v1/embeddings`.
pub fn build_request(request: &EmbeddingRequest, cfg: &OpenAiConfig) -> serde_json::Value {
    let mut body = serde_json::json!({
        "model": request.model.clone().unwrap_or_else(|| cfg.embedding_model.clone()),
        "input": request.input,
    });
    if let Some(dimensions) = request.dimensions {
        body["dimensions"] = serde_json::json!(dimensions);
    }
    body
}

#[derive(Debug, Deserialize)]
struct WireEmbeddingsResponse {
    model: String,
    #[serde(default)]
    data: Vec<WireEmbeddingData>,
    #[serde(default)]
    usage: Option<WireUsage>,
}

#[derive(Debug, Deserialize)]
struct WireEmbeddingData {
    index: usize,
    embedding: Vec<f32>,
}

#[derive(Debug, Deserialize)]
struct WireUsage {
    #[serde(default)]
    prompt_tokens: Option<u64>,
}

pub fn parse_response(body: &str) -> Result<EmbeddingResponse, LlmError> {
    let wire: WireEmbeddingsResponse =
        serde_json::from_str(body).map_err(|source| LlmError::Decode {
            provider: PROVIDER,
            context: "embeddings response".to_string(),
            source,
        })?;

    Ok(EmbeddingResponse {
        model: wire.model,
        embeddings: wire
            .data
            .into_iter()
            .map(|d| Embedding {
                index: d.index,
                vector: d.embedding,
            })
            .collect(),
        usage: wire.usage.map(|u| EmbeddingUsage {
            input_tokens: u.prompt_tokens,
        }),
    })
}

/// The substring OpenAI's own error message uses for an input that exceeds a
/// model's context length (observed wording: `"This model's maximum context
/// length is 8192 tokens, however you requested ... tokens"`). Matched
/// case-insensitively so a minor wording change doesn't silently stop being
/// caught.
const TOO_LONG_SIGNATURE: &str = "maximum context length";

/// Delegates to `wire::parse_error` for the general case, then upgrades a
/// 400/413 whose message names the context-length limit into
/// [`LlmError::InputTooLarge`] — every other error (auth, rate limit, an
/// unrelated 400) passes through unchanged.
pub fn parse_error(status: u16, body: &str, model: &str) -> LlmError {
    let err = wire::parse_error(status, body);
    match err {
        LlmError::Status {
            status: 400 | 413,
            message,
            ..
        } if message.to_lowercase().contains(TOO_LONG_SIGNATURE) => LlmError::InputTooLarge {
            provider: PROVIDER,
            model: model.to_string(),
            // OpenAI's error message states a limit, but not in a form worth
            // parsing back out — `ModelDetails::OpenAiEmbedding` never
            // reports one either (see its doc), so this stays consistent
            // with what the model listing already says.
            max_input_tokens: None,
            message,
        },
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EMBEDDINGS: &str = include_str!("fixtures/embeddings.json");
    const ERROR_CONTEXT_LENGTH: &str = include_str!("fixtures/error_context_length.json");
    const ERROR: &str = include_str!("fixtures/error.json");

    fn cfg() -> OpenAiConfig {
        OpenAiConfig::default()
    }

    #[test]
    fn builds_a_minimal_embeddings_request() {
        let req = EmbeddingRequest {
            input: vec!["hello world".to_string()],
            model: None,
            dimensions: None,
        };
        let body = build_request(&req, &cfg());
        assert_eq!(body["model"], serde_json::json!("text-embedding-3-small"));
        assert_eq!(body["input"], serde_json::json!(["hello world"]));
        assert!(body.get("dimensions").is_none());
    }

    #[test]
    fn a_requested_model_overrides_the_configured_default() {
        let req = EmbeddingRequest {
            input: vec!["hi".to_string()],
            model: Some("text-embedding-3-large".to_string()),
            dimensions: Some(256),
        };
        let body = build_request(&req, &cfg());
        assert_eq!(body["model"], serde_json::json!("text-embedding-3-large"));
        assert_eq!(body["dimensions"], serde_json::json!(256));
    }

    #[test]
    fn a_batch_of_inputs_is_sent_as_one_array() {
        let req = EmbeddingRequest {
            input: vec!["a".to_string(), "b".to_string(), "c".to_string()],
            model: None,
            dimensions: None,
        };
        let body = build_request(&req, &cfg());
        assert_eq!(body["input"], serde_json::json!(["a", "b", "c"]));
    }

    #[test]
    fn parses_an_embeddings_response_preserving_index_order() {
        let response = parse_response(EMBEDDINGS).unwrap();
        assert_eq!(response.model, "text-embedding-3-small");
        assert_eq!(response.embeddings.len(), 2);
        assert_eq!(response.embeddings[0].index, 0);
        assert_eq!(response.embeddings[1].index, 1);
        assert_eq!(response.embeddings[0].vector.len(), 4);
        assert_eq!(response.usage.unwrap().input_tokens, Some(6));
    }

    #[test]
    fn a_context_length_error_becomes_input_too_large() {
        let err = parse_error(400, ERROR_CONTEXT_LENGTH, "text-embedding-3-small");
        match err {
            LlmError::InputTooLarge { provider, model, .. } => {
                assert_eq!(provider, PROVIDER);
                assert_eq!(model, "text-embedding-3-small");
            }
            other => panic!("expected InputTooLarge, got {other:?}"),
        }
    }

    #[test]
    fn an_unrelated_error_stays_a_plain_status() {
        let err = parse_error(400, ERROR, "text-embedding-3-small");
        match err {
            LlmError::Status { status, .. } => assert_eq!(status, 400),
            other => panic!("expected Status, got {other:?}"),
        }
    }
}
