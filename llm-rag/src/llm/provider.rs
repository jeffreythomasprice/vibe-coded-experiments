use async_trait::async_trait;
use futures_util::stream::BoxStream;

use super::error::LlmError;
use super::types::{ChatRequest, ChatResponse, StreamChunk};

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, LlmError>;

    async fn chat_stream(
        &self,
        req: ChatRequest,
    ) -> Result<BoxStream<'static, Result<StreamChunk, LlmError>>, LlmError>;
}

#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, LlmError>;

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, LlmError>;

    /// Advertised max input-token context length, if the provider reports one.
    /// Used by the ingest pipeline to size chunks to the model's window.
    fn max_input_tokens(&self) -> Option<usize> {
        None
    }
}
