use std::collections::VecDeque;
use std::sync::Mutex;

use async_trait::async_trait;
use futures_util::stream::BoxStream;

use super::error::LlmError;
use super::provider::{EmbeddingProvider, LlmProvider};
use super::types::{ChatRequest, ChatResponse, StreamChunk};

/// An in-memory fake provider for tests. Queue up responses ahead of time;
/// requests received are kept in order for assertion. Never makes network
/// calls.
pub struct MockProvider {
    chat_responses: Mutex<VecDeque<Result<ChatResponse, LlmError>>>,
    stream_responses: Mutex<VecDeque<Vec<Result<StreamChunk, LlmError>>>>,
    embed_responses: Mutex<VecDeque<Result<Vec<f32>, LlmError>>>,
    pub received_requests: Mutex<Vec<ChatRequest>>,
    pub received_embed_texts: Mutex<Vec<String>>,
}

impl Default for MockProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MockProvider {
    pub fn new() -> Self {
        Self {
            chat_responses: Mutex::new(VecDeque::new()),
            stream_responses: Mutex::new(VecDeque::new()),
            embed_responses: Mutex::new(VecDeque::new()),
            received_requests: Mutex::new(Vec::new()),
            received_embed_texts: Mutex::new(Vec::new()),
        }
    }

    pub fn push_chat(&self, response: ChatResponse) {
        self.chat_responses.lock().unwrap().push_back(Ok(response));
    }

    pub fn push_chat_err(&self, err: LlmError) {
        self.chat_responses.lock().unwrap().push_back(Err(err));
    }

    pub fn push_stream(&self, chunks: Vec<StreamChunk>) {
        let wrapped = chunks.into_iter().map(Ok).collect();
        self.stream_responses.lock().unwrap().push_back(wrapped);
    }

    pub fn push_embedding(&self, vector: Vec<f32>) {
        self.embed_responses.lock().unwrap().push_back(Ok(vector));
    }
}

#[async_trait]
impl LlmProvider for MockProvider {
    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, LlmError> {
        self.received_requests.lock().unwrap().push(req);
        self.chat_responses
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| {
                Err(LlmError::Provider(anyhow::anyhow!(
                    "mock: no queued chat response"
                )))
            })
    }

    async fn chat_stream(
        &self,
        req: ChatRequest,
    ) -> Result<BoxStream<'static, Result<StreamChunk, LlmError>>, LlmError> {
        self.received_requests.lock().unwrap().push(req);
        let chunks = self
            .stream_responses
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_default();
        let mut chunks = chunks;
        chunks.push(Ok(StreamChunk::Done));
        Ok(Box::pin(futures_util::stream::iter(chunks)))
    }
}

#[async_trait]
impl EmbeddingProvider for MockProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, LlmError> {
        self.received_embed_texts
            .lock()
            .unwrap()
            .push(text.to_string());
        self.embed_responses
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| {
                Err(LlmError::Provider(anyhow::anyhow!(
                    "mock: no queued embedding"
                )))
            })
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, LlmError> {
        let mut out = Vec::with_capacity(texts.len());
        for t in texts {
            out.push(self.embed(t).await?);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::types::Message;
    use futures_util::StreamExt;

    #[tokio::test]
    async fn records_requests_and_returns_canned_response() {
        let mock = MockProvider::new();
        mock.push_chat(ChatResponse {
            text: "hello!".into(),
            tool_calls: vec![],
            structured: None,
        });

        let req = ChatRequest {
            messages: vec![Message::user("hi")],
            ..Default::default()
        };
        let resp = mock.chat(req).await.unwrap();
        assert_eq!(resp.text, "hello!");

        let received = mock.received_requests.lock().unwrap();
        assert_eq!(received.len(), 1);
        assert!(matches!(received[0].messages[0], Message::User { .. }));
    }

    #[tokio::test]
    async fn chat_stream_yields_queued_chunks_then_done() {
        let mock = MockProvider::new();
        mock.push_stream(vec![
            StreamChunk::Text("he".into()),
            StreamChunk::Text("llo".into()),
        ]);

        let req = ChatRequest {
            messages: vec![Message::user("hi")],
            ..Default::default()
        };
        let mut stream = mock.chat_stream(req).await.unwrap();
        let mut got = Vec::new();
        while let Some(chunk) = stream.next().await {
            got.push(chunk.unwrap());
        }
        assert_eq!(got.len(), 3);
        matches!(got[2], StreamChunk::Done);
    }

    #[tokio::test]
    async fn embed_returns_canned_vector() {
        let mock = MockProvider::new();
        mock.push_embedding(vec![0.1, 0.2, 0.3]);

        let v = mock.embed("hello").await.unwrap();
        assert_eq!(v, vec![0.1, 0.2, 0.3]);
        assert_eq!(
            mock.received_embed_texts.lock().unwrap().as_slice(),
            &["hello".to_string()]
        );
    }
}
