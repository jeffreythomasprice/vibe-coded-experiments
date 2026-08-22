//! Ollama's native `/api/embed` (batch) request/response shapes and pure
//! conversions to/from `shared::llm` types.
//!
//! Deliberately targets `/api/embed`, not the older single-input
//! `/api/embeddings` (`{"prompt": "..."}` in, `{"embedding": [f64...]}` out)
//! — `/api/embed` takes a batch and matches this crate's
//! `EmbeddingRequest::input: Vec<String>` shape directly.
//!
//! Every function here is pure — no I/O — so it's unit-tested directly on
//! recorded fixtures under `fixtures/`. See `ollama::mod` for the thin
//! transport layer that calls these.

use serde::Deserialize;

use shared::llm::{Embedding, EmbeddingRequest, EmbeddingResponse, EmbeddingUsage};

use crate::llm::config::OllamaConfig;
use crate::llm::error::LlmError;

use super::wire::PROVIDER;

/// Build the JSON body for `POST {base_url}/api/embed`. Model comes from
/// `request` — a required field on `EmbeddingRequest`, since it has no
/// sensible config-level default (see `crate::llm::config`'s module doc).
///
/// `truncate` is always sent as `false`, overriding Ollama's own default of
/// `true` (silently truncate oversized input to fit the context window).
/// Silently returning a vector for a shortened version of the input would be
/// a worse failure mode than an error — the caller would have no way to
/// know their embedding doesn't represent the whole string they sent. See
/// `parse_error` below for how the resulting rejection is classified.
pub fn build_request(request: &EmbeddingRequest, cfg: &OllamaConfig) -> serde_json::Value {
    let mut body = serde_json::json!({
        "model": request.model,
        "input": request.input,
        "truncate": false,
        "keep_alive": cfg.keep_alive,
    });
    if let Some(dimensions) = request.dimensions {
        // Ollama has no dimensions field on most embedding models, but a
        // handful of newer ones (e.g. embeddinggemma) do support Matryoshka
        // truncation the same way OpenAI's text-embedding-3-* models do —
        // pass it through when the caller asks, and let the model that
        // doesn't support it ignore an unrecognized field, same as
        // `options`/`keep_alive` elsewhere in this crate.
        body["dimensions"] = serde_json::json!(dimensions);
    }
    body
}

#[derive(Debug, Deserialize)]
struct WireEmbedResponse {
    model: String,
    #[serde(default)]
    embeddings: Vec<Vec<f32>>,
    #[serde(default)]
    prompt_eval_count: Option<u64>,
}

pub fn parse_response(body: &str) -> Result<EmbeddingResponse, LlmError> {
    let wire: WireEmbedResponse = serde_json::from_str(body).map_err(|source| LlmError::Decode {
        provider: PROVIDER,
        context: "embed response".to_string(),
        source,
    })?;

    Ok(EmbeddingResponse {
        model: wire.model,
        embeddings: wire
            .embeddings
            .into_iter()
            .enumerate()
            .map(|(index, vector)| Embedding { index, vector })
            .collect(),
        usage: wire.prompt_eval_count.map(|input_tokens| EmbeddingUsage {
            input_tokens: Some(input_tokens),
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

    const EMBED: &str = include_str!("fixtures/embed.json");

    fn cfg() -> OllamaConfig {
        OllamaConfig::default()
    }

    #[test]
    fn builds_a_minimal_embed_request_with_truncate_disabled() {
        let req = EmbeddingRequest::new("nomic-embed-text", vec!["hello world".to_string()]);
        let body = build_request(&req, &cfg());
        assert_eq!(body["model"], serde_json::json!("nomic-embed-text"));
        assert_eq!(body["input"], serde_json::json!(["hello world"]));
        assert_eq!(
            body["truncate"],
            serde_json::json!(false),
            "truncate must always be sent as false so oversized input errors \
             rather than silently returning a vector for a shortened string"
        );
        assert!(body.get("dimensions").is_none());
    }

    #[test]
    fn a_requested_model_lands_on_the_wire() {
        let req = EmbeddingRequest::new("mxbai-embed-large", vec!["hi".to_string()]);
        let body = build_request(&req, &cfg());
        assert_eq!(body["model"], serde_json::json!("mxbai-embed-large"));
    }

    #[test]
    fn a_batch_of_inputs_is_sent_as_one_array() {
        let req = EmbeddingRequest::new(
            "nomic-embed-text",
            vec!["a".to_string(), "b".to_string()],
        );
        let body = build_request(&req, &cfg());
        assert_eq!(body["input"], serde_json::json!(["a", "b"]));
    }

    #[test]
    fn parses_an_embed_response_assigning_indexes_by_array_position() {
        let response = parse_response(EMBED).unwrap();
        assert_eq!(response.model, "nomic-embed-text");
        assert_eq!(response.embeddings.len(), 2);
        assert_eq!(response.embeddings[0].index, 0);
        assert_eq!(response.embeddings[1].index, 1);
        assert_eq!(response.embeddings[0].vector.len(), 4);
        assert_eq!(response.usage.unwrap().input_tokens, Some(12));
    }

    // The context-length/billing reclassification tests that used to live
    // here moved to `lib/src/llm/error.rs`, alongside `ApiError::classify` —
    // they now go through `wire::parse_error_for_model` directly, since that
    // (not a per-endpoint wrapper) is what `embed()` calls.
}
