//! HTTP transport for Ollama's native chat API (`POST {base_url}/api/chat`).
//!
//! This module only does HTTP: request building, response parsing, and
//! error mapping all live in [`wire`] as pure functions over strings, so
//! that's where the unit tests are. What's here is exercised by
//! `lib/tests/live_ollama.rs`.
//!
//! Unlike Anthropic and OpenAI, there is no API key here — a local Ollama
//! install has none to send.

pub mod embeddings;
pub mod library;
pub mod wire;

use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::Client;
use std::time::{Duration, SystemTime};

use shared::llm::{
    ChatOptions, CompletedMessage, Conversation, EmbeddingRequest, EmbeddingResponse, ModelInfo,
};

use crate::cache::Cache;
use crate::llm::config::OllamaConfig;
use crate::llm::error::LlmError;
use crate::llm::http;
use crate::llm::ndjson::NdjsonDecoder;
use crate::llm::{ChatProvider, ChatStream, EmbeddingProvider, Freshness, ModelProvider};

pub struct OllamaClient {
    client: Client,
    cfg: OllamaConfig,
    max_retries: u32,
    cache: Cache,
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
            cache: Cache::disabled(),
        })
    }

    /// Opt into caching `list_models`/`list_models_remote`/`list_remote_tags`
    /// results — see `crate::cache`'s module doc. Not wired to any
    /// construction site by default.
    pub fn with_cache(mut self, cache: Cache) -> Self {
        self.cache = cache;
        self
    }

    fn endpoint(&self) -> String {
        format!("{}/api/chat", self.cfg.base_url.trim_end_matches('/'))
    }

    fn request(&self, body: &serde_json::Value) -> reqwest::RequestBuilder {
        self.client.post(self.endpoint()).json(body)
    }

    fn embed_endpoint(&self) -> String {
        format!("{}/api/embed", self.cfg.base_url.trim_end_matches('/'))
    }

    fn embed_request(&self, body: &serde_json::Value) -> reqwest::RequestBuilder {
        self.client.post(self.embed_endpoint()).json(body)
    }

    /// A plain `GET` with retry, shared by `list_models` (the local
    /// `/api/tags`) and the two `ollama.com/library` scrape methods below —
    /// no auth on any of them, like every other Ollama request.
    async fn get_text(
        &self,
        url: String,
        parse_error: impl Fn(u16, &str) -> LlmError,
    ) -> Result<String, LlmError> {
        let request = self.client.get(url);
        let response = http::send_with_retry(wire::PROVIDER, request, self.max_retries, parse_error).await?;
        response.text().await.map_err(|source| LlmError::Http {
            provider: wire::PROVIDER,
            source,
        })
    }

    /// Models available to pull from `ollama.com/library` — everything the
    /// library index lists, not only what `list_models`/`GET /api/tags`
    /// reports as already pulled. There's no JSON API behind this (see
    /// `library`'s module doc), so the response is cached under
    /// `ollama-library.html`.
    pub async fn list_models_remote(&self, freshness: Freshness) -> Result<Vec<ModelInfo>, LlmError> {
        const CACHE_KEY: &str = "ollama-library.html";

        if freshness == Freshness::Cached {
            if let Some(html) = self.cache.read(CACHE_KEY, SystemTime::now()) {
                if let Ok(models) = library::parse_library(&html) {
                    return Ok(models);
                }
                tracing::debug!("ignoring a cached ollama library page that failed to parse");
            }
        }

        let html = self.get_text(self.cfg.library_url.clone(), library::parse_error).await?;
        let models = library::parse_library(&html)?;
        self.cache.write(CACHE_KEY, &html);
        Ok(models)
    }

    /// Every pullable tag for one library model, e.g. `llama3.1:8b`,
    /// `llama3.1:405b-instruct-q4_K_M` — from
    /// `{library_url}/<model>/tags`. Cached per model, since a caller
    /// typically only wants tags for the one model they're about to pull.
    pub async fn list_remote_tags(
        &self,
        model: &str,
        freshness: Freshness,
    ) -> Result<Vec<ModelInfo>, LlmError> {
        let cache_key = format!("ollama-library-{}.html", crate::cache::sanitize_key(model));

        if freshness == Freshness::Cached {
            if let Some(html) = self.cache.read(&cache_key, SystemTime::now()) {
                if let Ok(models) = library::parse_tags_page(model, &html) {
                    return Ok(models);
                }
                tracing::debug!(model, "ignoring a cached ollama tags page that failed to parse");
            }
        }

        let url = format!("{}/{model}/tags", self.cfg.library_url.trim_end_matches('/'));
        let html = self.get_text(url, library::parse_error).await?;
        let models = library::parse_tags_page(model, &html)?;
        self.cache.write(&cache_key, &html);
        Ok(models)
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

#[async_trait]
impl ModelProvider for OllamaClient {
    fn name(&self) -> &'static str {
        wire::PROVIDER
    }

    /// Models already pulled and ready to run — `GET {base_url}/api/tags`.
    /// See [`OllamaClient::list_models_remote`] for what's available to pull
    /// but not yet local.
    async fn list_models(&self, freshness: Freshness) -> Result<Vec<ModelInfo>, LlmError> {
        const CACHE_KEY: &str = "ollama-tags.json";

        if freshness == Freshness::Cached {
            if let Some(cached) = self.cache.read(CACHE_KEY, SystemTime::now()) {
                if let Ok(models) = wire::parse_tags(&cached) {
                    return Ok(models);
                }
                tracing::debug!("ignoring a cached ollama tags response that failed to parse");
            }
        }

        let url = format!("{}/api/tags", self.cfg.base_url.trim_end_matches('/'));
        let text = self.get_text(url, wire::parse_error).await?;
        let models = wire::parse_tags(&text)?;
        self.cache.write(CACHE_KEY, &text);
        Ok(models)
    }
}

#[async_trait]
impl EmbeddingProvider for OllamaClient {
    fn name(&self) -> &'static str {
        wire::PROVIDER
    }

    /// Reuses `ModelProvider::list_models` — the same `/api/tags` listing
    /// already classifies embedding models via their `"embedding"`
    /// capability, so there's no separate endpoint or cache entry to
    /// maintain here.
    async fn list_embedding_models(
        &self,
        freshness: Freshness,
    ) -> Result<Vec<ModelInfo>, LlmError> {
        let models = ModelProvider::list_models(self, freshness).await?;
        Ok(crate::llm::embedding_models(models))
    }

    async fn embed(&self, request: &EmbeddingRequest) -> Result<EmbeddingResponse, LlmError> {
        let model = request
            .model
            .clone()
            .unwrap_or_else(|| self.cfg.embedding_model.clone());
        let body = embeddings::build_request(request, &self.cfg);
        let http_request = self.embed_request(&body);
        let response = http::send_with_retry(wire::PROVIDER, http_request, self.max_retries, {
            let model = model.clone();
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
