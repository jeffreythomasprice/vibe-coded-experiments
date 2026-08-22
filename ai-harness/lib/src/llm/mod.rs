//! LLM provider integrations: Anthropic, Ollama, and OpenAI, plus the
//! transport-agnostic plumbing they share.
//!
//! Each provider lives in its own submodule and is split into a `wire`
//! module (pure request-building and response/error parsing over `&str` /
//! `&[u8]`, unit-tested on recorded fixtures — no network) and a thin `mod.rs`
//! that only does HTTP. There is deliberately no router here: callers
//! construct the concrete client they want and talk to it through
//! [`ChatProvider`] or [`ImageProvider`] directly.
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
use shared::llm::{ChatOptions, CompletedMessage, Conversation, GeneratedImage, ImageRequest, StreamEvent};

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
