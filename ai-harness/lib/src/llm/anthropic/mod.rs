//! HTTP transport for the Anthropic Messages API (`POST {base_url}/v1/messages`).
//!
//! This module only does HTTP: request building, response parsing, and
//! error mapping all live in [`wire`] as pure functions over strings, so
//! that's where the unit tests are. What's here is exercised by
//! `lib/tests/live_anthropic.rs`.

pub mod wire;

use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::Client;
use std::time::Duration;

use shared::llm::{ChatOptions, CompletedMessage, Conversation};

use crate::llm::config::AnthropicConfig;
use crate::llm::error::LlmError;
use crate::llm::http;
use crate::llm::sse::SseDecoder;
use crate::llm::{ChatProvider, ChatStream};

pub struct AnthropicClient {
    client: Client,
    cfg: AnthropicConfig,
    max_retries: u32,
}

impl AnthropicClient {
    pub fn new(
        cfg: AnthropicConfig,
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
        format!("{}/v1/messages", self.cfg.base_url.trim_end_matches('/'))
    }

    fn request(&self, body: &serde_json::Value) -> Result<reqwest::RequestBuilder, LlmError> {
        let api_key = self.cfg.api_key()?;
        Ok(self
            .client
            .post(self.endpoint())
            .header("x-api-key", api_key)
            .header("anthropic-version", self.cfg.anthropic_version.clone())
            .json(body))
    }
}

#[async_trait]
impl ChatProvider for AnthropicClient {
    fn name(&self) -> &'static str {
        wire::PROVIDER
    }

    async fn complete(
        &self,
        conversation: &Conversation,
        options: &ChatOptions,
    ) -> Result<CompletedMessage, LlmError> {
        let body = wire::build_request(conversation, options, &self.cfg, false);
        let request = self.request(&body)?;
        let response =
            http::send_with_retry(wire::PROVIDER, request, self.max_retries, wire::parse_error)
                .await?;
        let text = response
            .text()
            .await
            .map_err(|source| LlmError::Http {
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
        let request = self.request(&body)?;
        let response =
            http::send_with_retry(wire::PROVIDER, request, self.max_retries, wire::parse_error)
                .await?;

        let stream = async_stream::try_stream! {
            let mut byte_stream = response.bytes_stream();
            let mut decoder = SseDecoder::new();
            let mut state = wire::StreamState::default();
            while let Some(chunk) = byte_stream.next().await {
                let chunk = chunk.map_err(|source| LlmError::Http {
                    provider: wire::PROVIDER,
                    source,
                })?;
                for frame in decoder.push(&chunk) {
                    if let Some(event) = wire::translate_frame(&frame, &mut state)? {
                        yield event;
                    }
                }
            }
        };

        Ok(Box::pin(stream))
    }
}
