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
use std::time::{Duration, SystemTime};

use shared::llm::{ChatOptions, CompletedMessage, Conversation, ModelInfo};

use crate::cache::Cache;
use crate::llm::config::AnthropicConfig;
use crate::llm::error::LlmError;
use crate::llm::http;
use crate::llm::sse::SseDecoder;
use crate::llm::{ChatProvider, ChatStream, Freshness, ModelProvider};

/// A hard cap on how many `/v1/models` pages [`AnthropicClient::list_models`]
/// will follow. At the maximum `limit` of 1000 this covers 20,000 models —
/// far more than Anthropic has ever listed — so this only exists to stop a
/// server that never clears `has_more` from spinning forever.
const MAX_MODEL_PAGES: u32 = 20;

pub struct AnthropicClient {
    client: Client,
    cfg: AnthropicConfig,
    max_retries: u32,
    cache: Cache,
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
            cache: Cache::disabled(),
        })
    }

    /// Opt into caching `list_models` results — see `crate::cache`'s module
    /// doc. Not wired to any construction site by default.
    pub fn with_cache(mut self, cache: Cache) -> Self {
        self.cache = cache;
        self
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

    fn get_request(&self, path_and_query: &str) -> Result<reqwest::RequestBuilder, LlmError> {
        let api_key = self.cfg.api_key()?;
        let url = format!("{}{path_and_query}", self.cfg.base_url.trim_end_matches('/'));
        Ok(self
            .client
            .get(url)
            .header("x-api-key", api_key)
            .header("anthropic-version", self.cfg.anthropic_version.clone()))
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
        let body = wire::build_request(conversation, options, false);
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
        let body = wire::build_request(conversation, options, true);
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

#[async_trait]
impl ModelProvider for AnthropicClient {
    fn name(&self) -> &'static str {
        wire::PROVIDER
    }

    /// Follows `has_more`/`last_id` pagination to assemble the full list
    /// (capped at [`MAX_MODEL_PAGES`]), then caches the assembled result
    /// under one key — a page boundary is an implementation detail of one
    /// listing, not something a cache read should have to know about.
    async fn list_models(&self, freshness: Freshness) -> Result<Vec<ModelInfo>, LlmError> {
        const CACHE_KEY: &str = "anthropic-models.json";

        if freshness == Freshness::Cached {
            if let Some(cached) = self.cache.read(CACHE_KEY, SystemTime::now()) {
                match serde_json::from_str::<Vec<ModelInfo>>(&cached) {
                    Ok(models) => return Ok(models),
                    Err(err) => {
                        tracing::debug!(%err, "ignoring a cached anthropic model list that failed to parse");
                    }
                }
            }
        }

        let mut models = Vec::new();
        let mut after_id: Option<String> = None;
        for _ in 0..MAX_MODEL_PAGES {
            let mut path = "/v1/models?limit=1000".to_string();
            if let Some(id) = &after_id {
                path.push_str("&after_id=");
                path.push_str(id);
            }
            let request = self.get_request(&path)?;
            let response =
                http::send_with_retry(wire::PROVIDER, request, self.max_retries, wire::parse_error)
                    .await?;
            let text = response.text().await.map_err(|source| LlmError::Http {
                provider: wire::PROVIDER,
                source,
            })?;
            let page = wire::parse_models(&text)?;
            models.extend(page.models);
            match page.next_after_id {
                Some(id) => after_id = Some(id),
                None => break,
            }
        }

        if let Ok(body) = serde_json::to_string(&models) {
            self.cache.write(CACHE_KEY, &body);
        }

        Ok(models)
    }
}
