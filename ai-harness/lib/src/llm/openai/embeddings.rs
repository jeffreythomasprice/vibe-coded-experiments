//! OpenAI Embeddings API (`POST {base_url}/v1/embeddings`) request/response
//! shapes and pure conversions to/from `shared::llm` types.
//!
//! Structured like `openai::images` — a second endpoint on the same client,
//! its own pure request/response/error functions, unit-tested on fixtures
//! under `fixtures/`. See `openai::mod` for the thin transport layer.

use serde::Deserialize;

use shared::llm::{Embedding, EmbeddingRequest, EmbeddingResponse, EmbeddingUsage};

use crate::llm::error::LlmError;

use super::wire::{self, PROVIDER};

/// Build the JSON body for `POST {base_url}/v1/embeddings`. Model comes
/// from `request` — a required field on `EmbeddingRequest`, since it has no
/// sensible config-level default (see `crate::llm::config`'s module doc).
pub fn build_request(request: &EmbeddingRequest) -> serde_json::Value {
    let mut body = serde_json::json!({
        "model": request.model,
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

/// Substrings OpenAI's own error messages use for an input that exceeds a
/// model's context length. Older wording: `"This model's maximum context
/// length is 8192 tokens, however you requested ... tokens"`. Newer wording,
/// observed on the embeddings endpoint: `"Invalid 'input[0]': maximum input
/// length is 8192 tokens."` Matched case-insensitively so a minor wording
/// change doesn't silently stop being caught; kept as a list since OpenAI has
/// already changed this wording once.
const TOO_LONG_SIGNATURES: &[&str] = &["maximum context length", "maximum input length"];

fn names_too_long_input(message: &str) -> bool {
    let lower = message.to_lowercase();
    TOO_LONG_SIGNATURES.iter().any(|sig| lower.contains(sig))
}

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
        } if names_too_long_input(&message) => LlmError::InputTooLarge {
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
    const ERROR_INPUT_TOO_LONG: &str = include_str!("fixtures/error_input_too_long.json");
    const ERROR: &str = include_str!("fixtures/error.json");

    #[test]
    fn builds_a_minimal_embeddings_request() {
        let req = EmbeddingRequest::new("text-embedding-3-small", vec!["hello world".to_string()]);
        let body = build_request(&req);
        assert_eq!(body["model"], serde_json::json!("text-embedding-3-small"));
        assert_eq!(body["input"], serde_json::json!(["hello world"]));
        assert!(body.get("dimensions").is_none());
    }

    #[test]
    fn a_requested_model_lands_on_the_wire() {
        let req = EmbeddingRequest {
            dimensions: Some(256),
            ..EmbeddingRequest::new("text-embedding-3-large", vec!["hi".to_string()])
        };
        let body = build_request(&req);
        assert_eq!(body["model"], serde_json::json!("text-embedding-3-large"));
        assert_eq!(body["dimensions"], serde_json::json!(256));
    }

    #[test]
    fn a_batch_of_inputs_is_sent_as_one_array() {
        let req = EmbeddingRequest::new(
            "text-embedding-3-small",
            vec!["a".to_string(), "b".to_string(), "c".to_string()],
        );
        let body = build_request(&req);
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
    fn an_input_too_long_error_becomes_input_too_large() {
        let err = parse_error(400, ERROR_INPUT_TOO_LONG, "text-embedding-3-small");
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
