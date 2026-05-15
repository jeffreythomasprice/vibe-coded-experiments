//! Chat-completion provider abstraction. Mirrors the shape of `src/ollama.rs`
//! (Ollama embeddings) but for completions, with support for both Ollama's
//! `/api/chat` endpoint and Anthropic's `/v1/messages` endpoint.
//!
//! `complete()` takes an optional cancel watch so callers can abort a
//! long-running request (e.g. a slow Anthropic call) without waiting for it
//! to return.

use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use crate::config::{AnthropicConfig, Config, LlmProvider, OllamaConfig};

#[derive(thiserror::Error, Debug)]
pub enum LlmError {
    #[error("http request to {url}: {source}")]
    Http {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    #[error("{provider} returned status {status} for {url}: {body}")]
    Status {
        provider: &'static str,
        url: String,
        status: u16,
        body: String,
    },

    #[error("decoding response from {url}: {source}")]
    Decode {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    #[error("env var {var} not set (configured as anthropic.api_key_env)")]
    MissingApiKey { var: String },

    #[error("{provider} returned an empty completion for model {model:?}")]
    EmptyResponse {
        provider: &'static str,
        model: String,
    },

    #[error("llm request cancelled")]
    Cancelled,
}

/// Issue a single chat completion against the provider configured in `cfg.llm`.
///
/// `system` is the system prompt; `user` is the single user-turn message. If
/// `cancel` is provided and fires `true` before the HTTP call finishes, the
/// request is dropped and `LlmError::Cancelled` is returned.
pub async fn complete(
    http: &reqwest::Client,
    cfg: &Config,
    system: &str,
    user: &str,
    cancel: Option<&watch::Receiver<bool>>,
) -> Result<String, LlmError> {
    let max_tokens = cfg.llm.max_output_tokens;
    let fut = async {
        match cfg.llm.provider {
            LlmProvider::Ollama => ollama_chat(http, &cfg.ollama, system, user, max_tokens).await,
            LlmProvider::Anthropic => {
                anthropic_chat(http, &cfg.anthropic, system, user, max_tokens).await
            }
        }
    };
    match cancel {
        Some(rx) => {
            let mut rx = rx.clone();
            tokio::select! {
                res = fut => res,
                changed = rx.changed() => {
                    if changed.is_ok() && *rx.borrow() {
                        Err(LlmError::Cancelled)
                    } else {
                        // Watch channel closed without a cancel signal — fall
                        // through to a fresh await with no cancel.
                        match cfg.llm.provider {
                            LlmProvider::Ollama => ollama_chat(http, &cfg.ollama, system, user, max_tokens).await,
                            LlmProvider::Anthropic => anthropic_chat(http, &cfg.anthropic, system, user, max_tokens).await,
                        }
                    }
                }
            }
        }
        None => fut.await,
    }
}

#[derive(Serialize)]
struct OllamaChatRequest<'a> {
    model: &'a str,
    messages: Vec<OllamaChatMessage<'a>>,
    stream: bool,
    options: OllamaChatOptions,
}

#[derive(Serialize)]
struct OllamaChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct OllamaChatOptions {
    num_predict: u32,
}

#[derive(Deserialize)]
struct OllamaChatResponse {
    message: OllamaChatResponseMessage,
}

#[derive(Deserialize)]
struct OllamaChatResponseMessage {
    content: String,
}

async fn ollama_chat(
    http: &reqwest::Client,
    cfg: &OllamaConfig,
    system: &str,
    user: &str,
    max_tokens: u32,
) -> Result<String, LlmError> {
    let endpoint = format!("{}/api/chat", cfg.url.trim_end_matches('/'));
    let body = OllamaChatRequest {
        model: &cfg.chat_model,
        messages: vec![
            OllamaChatMessage {
                role: "system",
                content: system,
            },
            OllamaChatMessage {
                role: "user",
                content: user,
            },
        ],
        stream: false,
        options: OllamaChatOptions {
            num_predict: max_tokens,
        },
    };

    let resp = http
        .post(&endpoint)
        .json(&body)
        .send()
        .await
        .map_err(|source| LlmError::Http {
            url: endpoint.clone(),
            source,
        })?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(LlmError::Status {
            provider: "ollama",
            url: endpoint,
            status: status.as_u16(),
            body,
        });
    }

    let parsed: OllamaChatResponse = resp.json().await.map_err(|source| LlmError::Decode {
        url: endpoint,
        source,
    })?;
    let content = parsed.message.content.trim().to_string();
    if content.is_empty() {
        return Err(LlmError::EmptyResponse {
            provider: "ollama",
            model: cfg.chat_model.clone(),
        });
    }
    Ok(content)
}

#[derive(Serialize)]
struct AnthropicRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    system: &'a str,
    messages: Vec<AnthropicMessage<'a>>,
}

#[derive(Serialize)]
struct AnthropicMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContentBlock>,
}

#[derive(Deserialize)]
struct AnthropicContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    #[serde(default)]
    text: String,
}

async fn anthropic_chat(
    http: &reqwest::Client,
    cfg: &AnthropicConfig,
    system: &str,
    user: &str,
    max_tokens: u32,
) -> Result<String, LlmError> {
    let api_key =
        std::env::var(&cfg.api_key_env).map_err(|_| LlmError::MissingApiKey {
            var: cfg.api_key_env.clone(),
        })?;
    let endpoint = format!("{}/v1/messages", cfg.base_url.trim_end_matches('/'));
    let body = AnthropicRequest {
        model: &cfg.model,
        max_tokens,
        system,
        messages: vec![AnthropicMessage {
            role: "user",
            content: user,
        }],
    };

    let resp = http
        .post(&endpoint)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|source| LlmError::Http {
            url: endpoint.clone(),
            source,
        })?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(LlmError::Status {
            provider: "anthropic",
            url: endpoint,
            status: status.as_u16(),
            body,
        });
    }

    let parsed: AnthropicResponse = resp.json().await.map_err(|source| LlmError::Decode {
        url: endpoint,
        source,
    })?;
    let text = parsed
        .content
        .into_iter()
        .filter(|b| b.block_type == "text")
        .map(|b| b.text)
        .collect::<Vec<_>>()
        .join("");
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err(LlmError::EmptyResponse {
            provider: "anthropic",
            model: cfg.model.clone(),
        });
    }
    Ok(text)
}
