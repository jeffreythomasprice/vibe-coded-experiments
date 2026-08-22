//! Embedding types: a request to vectorize text and the vectors that come
//! back.
//!
//! Modeled on `image.rs` — the established shape for a non-chat capability's
//! request/response pair.

use serde::{Deserialize, Serialize};

/// A request to turn one or more strings into vectors.
///
/// `input` is always a batch — both OpenAI's `/v1/embeddings` and Ollama's
/// `/api/embed` take an array, and a single string is just a batch of one.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingRequest {
    pub input: Vec<String>,
    /// Which model to call. Required — see `ChatOptions::model`'s doc for
    /// why there's no configured default to fall back to.
    pub model: String,
    /// Only OpenAI's `text-embedding-3-*` models honor this (Matryoshka
    /// truncation); Ollama has no equivalent and ignores it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<u32>,
}

impl EmbeddingRequest {
    pub fn new(model: impl Into<String>, input: Vec<String>) -> Self {
        Self {
            input,
            model: model.into(),
            dimensions: None,
        }
    }
}

/// One input's vector, at `index` in the request's `input` array — providers
/// are not guaranteed to return results in request order, so this is how a
/// caller matches a vector back to the string it came from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Embedding {
    pub index: usize,
    /// `f32`, not `f64` — halves the payload, and matches what both
    /// providers send on the wire.
    pub vector: Vec<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingUsage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingResponse {
    pub model: String,
    pub embeddings: Vec<Embedding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<EmbeddingUsage>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedding_request_omits_absent_optional_fields() {
        let req = EmbeddingRequest::new("text-embedding-3-small", vec!["hello".to_string()]);
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["model"], serde_json::json!("text-embedding-3-small"));
        assert!(json.get("dimensions").is_none());
    }

    #[test]
    fn embedding_response_round_trips() {
        let response = EmbeddingResponse {
            model: "text-embedding-3-small".to_string(),
            embeddings: vec![
                Embedding {
                    index: 0,
                    vector: vec![0.1, -0.2, 0.3],
                },
                Embedding {
                    index: 1,
                    vector: vec![0.4, 0.5],
                },
            ],
            usage: Some(EmbeddingUsage {
                input_tokens: Some(12),
            }),
        };
        let json = serde_json::to_string(&response).unwrap();
        let back: EmbeddingResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.model, response.model);
        assert_eq!(back.embeddings, response.embeddings);
        assert_eq!(back.usage, response.usage);
    }

    #[test]
    fn embedding_response_omits_usage_when_absent() {
        let response = EmbeddingResponse {
            model: "nomic-embed-text".to_string(),
            embeddings: vec![],
            usage: None,
        };
        let json = serde_json::to_value(&response).unwrap();
        assert!(json.get("usage").is_none());
    }

    #[test]
    fn vector_serializes_as_f32_not_f64() {
        // f32 halves the payload versus f64 for the same vector length —
        // this is a compile-time guarantee, but pin the field type with a
        // round trip so a future edit can't silently widen it.
        let embedding = Embedding {
            index: 0,
            vector: vec![1.0_f32 / 3.0],
        };
        let json = serde_json::to_string(&embedding).unwrap();
        let back: Embedding = serde_json::from_str(&json).unwrap();
        assert_eq!(back.vector[0], 1.0_f32 / 3.0);
    }
}
