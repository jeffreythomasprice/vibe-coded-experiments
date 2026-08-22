//! OpenAI Embeddings API (`POST {base_url}/v1/embeddings`) request/response
//! shapes and pure conversions to/from `shared::llm` types.
//!
//! Structured like `openai::images` — a second endpoint on the same client,
//! its own pure request/response/error functions, unit-tested on fixtures
//! under `fixtures/`. See `openai::mod` for the thin transport layer.

use serde::Deserialize;

use shared::llm::{Embedding, EmbeddingRequest, EmbeddingResponse, EmbeddingUsage};

use crate::llm::error::LlmError;

use super::wire::PROVIDER;

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

// Error parsing for this endpoint is now handled by `wire::parse_error_for_model`
// (see `crate::llm::error::ApiError::classify`) — the same reclassification
// this module used to do on its own now applies to the chat path too. See
// `lib/src/llm/error.rs`'s tests for the context-length/billing coverage
// that used to live here.

#[cfg(test)]
mod tests {
    use super::*;

    const EMBEDDINGS: &str = include_str!("fixtures/embeddings.json");

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

    // The context-length/billing reclassification tests that used to live
    // here moved to `lib/src/llm/error.rs`, alongside `ApiError::classify` —
    // they now go through `wire::parse_error_for_model` directly, since that
    // (not a per-endpoint wrapper) is what `embed()` calls.
}
