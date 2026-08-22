//! LLM provider integrations: Anthropic, Ollama, and OpenAI, plus the
//! transport-agnostic plumbing they share.
//!
//! Each provider lives in its own submodule and is split into a `wire`
//! module (pure request-building and response/error parsing over `&str` /
//! `&[u8]`, unit-tested on recorded fixtures — no network) and a thin `mod.rs`
//! that only does HTTP. There is deliberately no router here: callers
//! construct the concrete client they want and talk to it through
//! [`ChatProvider`], [`ImageProvider`], or [`EmbeddingProvider`] directly.
//!
//! See `README.md`'s "Tests" section and `CLAUDE.md` for the unit-vs-live
//! testing split this module is built around.

pub mod accumulate;
pub mod anthropic;
pub mod config;
pub mod error;
pub mod http;
pub mod ndjson;
pub mod ollama;
pub mod openai;
pub mod sse;

pub use anthropic::AnthropicClient;
pub use config::LlmConfig;
pub use error::{ApiErrorKind, LlmError};
pub use ollama::OllamaClient;
pub use openai::OpenAiClient;

use std::pin::Pin;

use async_trait::async_trait;
use futures_util::Stream;
use shared::llm::{
    ChatOptions, CompletedMessage, Conversation, EmbeddingRequest, EmbeddingResponse,
    GeneratedImage, ImageRequest, ModelInfo, StreamEvent,
};

/// A stream of `StreamEvent`s from [`ChatProvider::stream`].
///
/// Boxed because each provider's underlying stream (built with
/// `async-stream`) is a distinct, non-nameable type — boxing behind a trait
/// object here is also what would let a future router hold
/// `Box<dyn ChatProvider>` without every call site knowing the concrete
/// provider.
pub type ChatStream = Pin<Box<dyn Stream<Item = Result<StreamEvent, LlmError>> + Send>>;

/// A provider that can carry on a conversation. Anthropic, Ollama, and
/// OpenAI all implement this.
#[async_trait]
pub trait ChatProvider: Send + Sync {
    /// Short name used in error messages and logs (e.g. `"anthropic"`).
    fn name(&self) -> &'static str;

    /// A single non-streaming request/response turn. Issues a real
    /// `stream: false` request rather than folding a stream internally, so
    /// this path stays as debuggable as a plain `curl` call.
    async fn complete(
        &self,
        conversation: &Conversation,
        options: &ChatOptions,
    ) -> Result<CompletedMessage, LlmError>;

    /// The same turn, streamed. Fold the result with
    /// [`accumulate::MessageAccumulator`] to get the same shape `complete`
    /// would have returned — every provider's live test asserts the two
    /// agree.
    async fn stream(
        &self,
        conversation: &Conversation,
        options: &ChatOptions,
    ) -> Result<ChatStream, LlmError>;
}

/// A provider that can generate images from a prompt.
///
/// Only `openai::OpenAiClient` implements this today: Ollama's
/// image-generation endpoint is macOS-only (verified against a local
/// install — see `ollama::wire`) and Anthropic doesn't offer one.
/// Capability discovery is by type, not a runtime flag — a caller that needs
/// image generation asks for something that implements `ImageProvider`, and
/// `AnthropicClient`/`OllamaClient` are simply not candidates.
#[async_trait]
pub trait ImageProvider: Send + Sync {
    fn name(&self) -> &'static str;

    async fn generate(&self, request: &ImageRequest) -> Result<Vec<GeneratedImage>, LlmError>;
}

/// Whether [`ModelProvider::list_models`] (and Ollama's `list_models_remote`
/// / `list_remote_tags`) may answer from the on-disk cache configured by
/// `[cache]`, or must hit the network.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Freshness {
    /// Reuse a cached list if it's younger than `[cache].ttl_secs`. The
    /// right default for "what models are there" — a model list changes
    /// rarely enough that a few hours of staleness is a fine trade for
    /// skipping the network.
    #[default]
    Cached,
    /// Ignore any cached entry and re-fetch, writing the fresh result back
    /// to the cache. Needed right after `ollama pull` — Ollama's own
    /// `/api/tags` would otherwise not show the new model until the cached
    /// entry expires.
    Refresh,
}

/// A provider that can report which models it offers. Anthropic, Ollama, and
/// OpenAI all implement this, following the same by-type capability
/// discovery [`ImageProvider`] already established. Ollama additionally
/// exposes two inherent (non-trait) methods,
/// `OllamaClient::list_models_remote` and `OllamaClient::list_remote_tags`,
/// for `ollama.com/library` — nothing else could implement them, since only
/// Ollama distinguishes "models pulled and ready to run" from "models
/// available to pull".
#[async_trait]
pub trait ModelProvider: Send + Sync {
    fn name(&self) -> &'static str;

    async fn list_models(&self, freshness: Freshness) -> Result<Vec<ModelInfo>, LlmError>;
}

/// A provider that can turn text into vectors. `OpenAiClient` and
/// `OllamaClient` implement this; `AnthropicClient` does not, following the
/// same by-type capability discovery [`ImageProvider`] established —
/// Anthropic's API surface is Messages, Batches, Files, Token Counting, and
/// Models, with no embeddings endpoint at all.
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    fn name(&self) -> &'static str;

    /// Every embeddings model this provider offers. Same shape as
    /// [`ModelProvider::list_models`] — every entry's
    /// `ModelDetails::is_embedding()` is `true`.
    async fn list_embedding_models(&self, freshness: Freshness)
        -> Result<Vec<ModelInfo>, LlmError>;

    async fn embed(&self, request: &EmbeddingRequest) -> Result<EmbeddingResponse, LlmError>;
}

/// Filter a provider's full model list down to embeddings models. Shared by
/// every [`EmbeddingProvider::list_embedding_models`] implementation so the
/// filter exists in one place rather than being re-derived per provider.
pub fn embedding_models(models: Vec<ModelInfo>) -> Vec<ModelInfo> {
    models
        .into_iter()
        .filter(|m| m.details.is_embedding())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::llm::ModelDetails;

    fn chat_model(id: &str) -> ModelInfo {
        ModelInfo {
            id: id.to_string(),
            display_name: None,
            created_at: None,
            details: ModelDetails::OpenAi {
                owned_by: "openai".to_string(),
            },
        }
    }

    fn embedding_model(id: &str) -> ModelInfo {
        ModelInfo {
            id: id.to_string(),
            display_name: None,
            created_at: None,
            details: ModelDetails::OpenAiEmbedding {
                owned_by: "openai".to_string(),
            },
        }
    }

    #[test]
    fn embedding_models_keeps_only_embedding_entries_in_order() {
        let models = vec![
            chat_model("gpt-5.6"),
            embedding_model("text-embedding-3-small"),
            chat_model("gpt-image-2"),
            embedding_model("text-embedding-3-large"),
        ];
        let filtered = embedding_models(models);
        let ids: Vec<&str> = filtered.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, vec!["text-embedding-3-small", "text-embedding-3-large"]);
    }

    #[test]
    fn embedding_models_is_empty_when_nothing_is_an_embedding_model() {
        let models = vec![chat_model("gpt-5.6")];
        assert!(embedding_models(models).is_empty());
    }
}
