//! A thin async client for the [Ollama](https://ollama.com) HTTP API.
//!
//! This is the one piece of the `ai` module that is **not** a stub: it makes
//! real HTTP calls. Agents wrap an `OllamaClient` (one per role, each with its
//! own model id) and call [`OllamaClient::generate`] to get a completion.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use shared::config::OllamaConfig;

/// A handle to an Ollama server bound to a single model id.
///
/// Cloning is cheap — `reqwest::Client` is an `Arc` internally — so each agent
/// can hold its own clone for its role's model.
#[derive(Debug, Clone)]
pub struct OllamaClient {
    http: reqwest::Client,
    /// Base URL of the Ollama API, e.g. `http://localhost:11434`.
    base_url: String,
    /// The model id this client requests, e.g. `qwen3.5`.
    model: String,
}

impl OllamaClient {
    /// Build a client for `model` against the server at `base_url`
    /// (e.g. `http://localhost:11434`).
    pub fn new(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        OllamaClient {
            http: reqwest::Client::new(),
            base_url: base_url.into(),
            model: model.into(),
        }
    }

    /// Build a client from the loaded [`OllamaConfig`] for an explicit `model`.
    /// Callers pass the per-role model id (`player_model` / `referee_model`) so
    /// the connection details and the model choice come from one place.
    pub fn from_config(config: &OllamaConfig, model: impl Into<String>) -> Self {
        Self::new(config.base_url(), model)
    }

    /// The model id this client targets.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Run a single non-streaming completion against `POST /api/generate` and
    /// return the model's `response` text.
    pub async fn generate(&self, prompt: &str) -> Result<String> {
        let url = format!("{}/api/generate", self.base_url);
        let body = GenerateRequest {
            model: &self.model,
            prompt,
            stream: false,
        };

        let resp = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .with_context(|| format!("POST {url}"))?
            .error_for_status()
            .with_context(|| format!("Ollama returned an error status from {url}"))?
            .json::<GenerateResponse>()
            .await
            .context("decoding Ollama /api/generate response")?;

        Ok(resp.response)
    }

    /// Probe whether the server is reachable by hitting `GET /api/tags`. Used
    /// for diagnostics/logging; not required before [`generate`](Self::generate).
    pub async fn health(&self) -> Result<()> {
        let url = format!("{}/api/tags", self.base_url);
        self.http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {url}"))?
            .error_for_status()
            .with_context(|| format!("Ollama health check failed at {url}"))?;
        Ok(())
    }
}

/// The subset of Ollama's `/api/generate` request body we use.
#[derive(Serialize)]
struct GenerateRequest<'a> {
    model: &'a str,
    prompt: &'a str,
    /// We always want the full completion in one response, not a token stream.
    stream: bool,
}

/// The subset of Ollama's `/api/generate` response we care about.
#[derive(Deserialize)]
struct GenerateResponse {
    response: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Live smoke test against a local Ollama. Ignored by default (needs a
    /// running server + the model pulled). Run with:
    /// `cargo test -p engine -- --ignored ollama_generate_live`
    #[tokio::test]
    #[ignore = "requires a running Ollama server"]
    async fn ollama_generate_live() {
        let client = OllamaClient::new("http://localhost:11434", "qwen3.5");
        client.health().await.expect("ollama reachable");
        let reply = client
            .generate("Reply with the single word: pong")
            .await
            .expect("generate succeeds");
        assert!(!reply.is_empty(), "expected a non-empty completion");
    }
}
