//! HTTP transport for OpenAI: chat via the Responses API
//! (`POST {base_url}/v1/responses`) and image generation
//! (`POST {base_url}/v1/images/generations`).
//!
//! This module only does HTTP: request building, response parsing, and
//! error mapping all live in [`wire`] (chat) and [`images`] (image
//! generation) as pure functions over strings, so that's where the unit
//! tests are. What's here is exercised by `lib/tests/live_openai.rs`.
//!
//! `OpenAiClient` is the only client in this crate that implements both
//! [`ChatProvider`] and [`ImageProvider`] — image generation is the
//! capability that makes OpenAI worth having as a third provider, per this
//! project's requirements.

pub mod embeddings;
pub mod images;
pub mod wire;

use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::Client;
use std::time::{Duration, SystemTime};

use shared::llm::{
    ChatOptions, CompletedMessage, Conversation, EmbeddingRequest, EmbeddingResponse,
    GeneratedImage, ImageRequest, ModelInfo,
};

use crate::cache::Cache;
use crate::llm::config::OpenAiConfig;
use crate::llm::error::LlmError;
use crate::llm::http;
use crate::llm::sse::SseDecoder;
use crate::llm::{ChatProvider, ChatStream, EmbeddingProvider, Freshness, ImageProvider, ModelProvider};

pub struct OpenAiClient {
    client: Client,
    cfg: OpenAiConfig,
    max_retries: u32,
    cache: Cache,
}

impl OpenAiClient {
    pub fn new(
        cfg: OpenAiConfig,
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

    fn endpoint(&self, path: &str) -> String {
        format!("{}{path}", self.cfg.base_url.trim_end_matches('/'))
    }

    fn request(&self, path: &str, body: &serde_json::Value) -> Result<reqwest::RequestBuilder, LlmError> {
        let api_key = self.cfg.api_key()?;
        Ok(self
            .client
            .post(self.endpoint(path))
            .bearer_auth(api_key)
            .json(body))
    }

    fn get_request(&self, path: &str) -> Result<reqwest::RequestBuilder, LlmError> {
        let api_key = self.cfg.api_key()?;
        Ok(self.client.get(self.endpoint(path)).bearer_auth(api_key))
    }
}

#[async_trait]
impl ChatProvider for OpenAiClient {
    fn name(&self) -> &'static str {
        wire::PROVIDER
    }

    async fn complete(
        &self,
        conversation: &Conversation,
        options: &ChatOptions,
    ) -> Result<CompletedMessage, LlmError> {
        let body = wire::build_request(conversation, options, false);
        let request = self.request("/v1/responses", &body)?;
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
        let body = wire::build_request(conversation, options, true);
        let request = self.request("/v1/responses", &body)?;
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
impl ImageProvider for OpenAiClient {
    fn name(&self) -> &'static str {
        wire::PROVIDER
    }

    async fn generate(&self, request: &ImageRequest) -> Result<Vec<GeneratedImage>, LlmError> {
        let body = images::build_request(request);
        let http_request = self.request("/v1/images/generations", &body)?;
        let response = http::send_with_retry(
            wire::PROVIDER,
            http_request,
            self.max_retries,
            wire::parse_error,
        )
        .await?;
        let text = response.text().await.map_err(|source| LlmError::Http {
            provider: wire::PROVIDER,
            source,
        })?;
        images::parse_response(&text, request.output_format.as_deref())
    }
}

#[async_trait]
impl ModelProvider for OpenAiClient {
    fn name(&self) -> &'static str {
        wire::PROVIDER
    }

    async fn list_models(&self, freshness: Freshness) -> Result<Vec<ModelInfo>, LlmError> {
        // `-v2`: this cache stores the *parsed* `Vec<ModelInfo>`, and a list
        // cached under the old key would still deserialize fine but classify
        // every model as plain `OpenAi` — bumping the key means the new
        // chat/embedding/image classification takes effect immediately
        // rather than waiting out the TTL.
        const CACHE_KEY: &str = "openai-models-v2.json";

        if freshness == Freshness::Cached {
            if let Some(cached) = self.cache.read(CACHE_KEY, SystemTime::now()) {
                match serde_json::from_str::<Vec<ModelInfo>>(&cached) {
                    Ok(models) => return Ok(models),
                    Err(err) => {
                        tracing::debug!(%err, "ignoring a cached openai model list that failed to parse");
                    }
                }
            }
        }

        let request = self.get_request("/v1/models")?;
        let response =
            http::send_with_retry(wire::PROVIDER, request, self.max_retries, wire::parse_error)
                .await?;
        let text = response.text().await.map_err(|source| LlmError::Http {
            provider: wire::PROVIDER,
            source,
        })?;
        let models = wire::parse_models(&text)?;

        if let Ok(body) = serde_json::to_string(&models) {
            self.cache.write(CACHE_KEY, &body);
        }

        Ok(models)
    }
}

#[async_trait]
impl EmbeddingProvider for OpenAiClient {
    fn name(&self) -> &'static str {
        wire::PROVIDER
    }

    /// Reuses `ModelProvider::list_models` — the same `/v1/models` listing
    /// already classifies embedding models via `wire::classify`, so there's
    /// no separate endpoint or cache entry to maintain here.
    async fn list_embedding_models(
        &self,
        freshness: Freshness,
    ) -> Result<Vec<ModelInfo>, LlmError> {
        let models = ModelProvider::list_models(self, freshness).await?;
        Ok(crate::llm::embedding_models(models))
    }

    async fn embed(&self, request: &EmbeddingRequest) -> Result<EmbeddingResponse, LlmError> {
        let body = embeddings::build_request(request);
        let http_request = self.request("/v1/embeddings", &body)?;
        let response = http::send_with_retry(wire::PROVIDER, http_request, self.max_retries, {
            let model = request.model.clone();
            move |status, body| embeddings::parse_error(status, body, &model)
        })
        .await?;
        let text = response.text().await.map_err(|source| LlmError::Http {
            provider: wire::PROVIDER,
            source,
        })?;
        embeddings::parse_response(&text)
    }
}
