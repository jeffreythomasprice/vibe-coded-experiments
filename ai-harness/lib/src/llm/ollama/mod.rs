//! HTTP transport for Ollama's native chat API (`POST {base_url}/api/chat`).
//!
//! This module only does HTTP: request building, response parsing, and
//! error mapping all live in [`wire`] as pure functions over strings, so
//! that's where the unit tests are. What's here is exercised by
//! `lib/tests/live_ollama.rs`.
//!
//! Unlike Anthropic and OpenAI, there is no API key here — a local Ollama
//! install has none to send.

pub mod wire;

use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::Client;
use std::time::Duration;

use shared::llm::{ChatOptions, CompletedMessage, Conversation};

use crate::llm::config::OllamaConfig;
use crate::llm::error::LlmError;
use crate::llm::http;
use crate::llm::ndjson::NdjsonDecoder;
use crate::llm::{ChatProvider, ChatStream};

pub struct OllamaClient {
    client: Client,
    cfg: OllamaConfig,
    max_retries: u32,
}

impl OllamaClient {
    pub fn new(
        cfg: OllamaConfig,
        request_timeout: Duration,
        max_retries: u32,
    ) -> Result<Self, LlmError> {
        Ok(Self {
            client: http::build_client(request_timeout)?,
            cfg,
            max_retries,
        })
    }

    fn endpoint(&self) -> String {
        format!("{}/api/chat", self.cfg.base_url.trim_end_matches('/'))
    }

    fn request(&self, body: &serde_json::Value) -> reqwest::RequestBuilder {
        self.client.post(self.endpoint()).json(body)
    }
}

#[async_trait]
impl ChatProvider for OllamaClient {
    fn name(&self) -> &'static str {
        wire::PROVIDER
    }

    async fn complete(
        &self,
        conversation: &Conversation,
        options: &ChatOptions,
    ) -> Result<CompletedMessage, LlmError> {
        let body = wire::build_request(conversation, options, &self.cfg, false);
        let request = self.request(&body);
        let response =
            http::send_with_retry(wire::PROVIDER, request, self.max_retries, wire::parse_error)
                .await?;
        let text = response.text().await.map_err(|source| LlmError::Http {
            provider: wire::PROVIDER,
            source,
        })?;
        wire::parse_response(&text)
    }

    async fn stream(
        &self,
        conversation: &Conversation,
        options: &ChatOptions,
    ) -> Result<ChatStream, LlmError> {
        let body = wire::build_request(conversation, options, &self.cfg, true);
        let request = self.request(&body);
        let response =
            http::send_with_retry(wire::PROVIDER, request, self.max_retries, wire::parse_error)
                .await?;

        let stream = async_stream::try_stream! {
            let mut byte_stream = response.bytes_stream();
            let mut decoder = NdjsonDecoder::new();
            let mut state = wire::StreamState::default();
            while let Some(chunk) = byte_stream.next().await {
                let chunk = chunk.map_err(|source| LlmError::Http {
                    provider: wire::PROVIDER,
                    source,
                })?;
                for line in decoder.push(&chunk) {
                    for event in wire::translate_line(&line, &mut state)? {
                        yield event;
                    }
                }
            }
        };

        Ok(Box::pin(stream))
    }
}
