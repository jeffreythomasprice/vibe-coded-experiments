//! Chat-completion provider abstraction. Mirrors the shape of `src/ollama.rs`
//! (Ollama embeddings) but for completions, with support for both Ollama's
//! `/api/chat` endpoint and Anthropic's `/v1/messages` endpoint.
//!
//! `complete()` takes an optional cancel watch so callers can abort a
//! long-running request (e.g. a slow Anthropic call) without waiting for it
//! to return.

use base64::Engine as _;
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

    #[error(
        "{provider} model {model:?} spent its whole {max_tokens}-token budget thinking and \
         produced no answer; raise [llm].max_output_tokens or set [ollama].think = false"
    )]
    ThinkingExhausted {
        provider: &'static str,
        model: String,
        max_tokens: u32,
    },

    #[error(
        "prompt is ~{estimated_tokens} tokens, which leaves no room for output in the \
         {num_ctx}-token context; raise [ollama].num_ctx or lower summarize.group_size"
    )]
    PromptTooLarge {
        estimated_tokens: u32,
        num_ctx: u32,
    },

    #[error(
        "ollama model {model:?} only saw {prompt_tokens} of ~{estimated_tokens} prompt \
         tokens, so the prompt was truncated (context {num_ctx}); raise [ollama].num_ctx \
         or check that the Ollama server honors num_ctx"
    )]
    PromptTruncated {
        model: String,
        prompt_tokens: u32,
        estimated_tokens: u32,
        num_ctx: u32,
    },

    #[error("llm request cancelled")]
    Cancelled,
}

/// Rough upper-bound token count for sizing a request: one token per non-ASCII
/// char (CJK tokenizes at roughly a char per token or better) and one per
/// four ASCII bytes. Deliberately pessimistic so the context is never short.
pub fn estimate_tokens(s: &str) -> u32 {
    let (ascii, other) = s
        .chars()
        .fold((0u32, 0u32), |(a, o), c| if c.is_ascii() { (a + 1, o) } else { (a, o + 1) });
    other + ascii.div_ceil(4)
}

/// Headroom on top of the message text for the chat template's role markers
/// and special tokens.
const TEMPLATE_OVERHEAD_TOKENS: u32 = 64;

/// The smallest output budget worth sending a request for. A prompt that
/// leaves less than this in the context fails up front with `PromptTooLarge`.
const MIN_OUTPUT_TOKENS: u32 = 1024;

/// What one attached image is assumed to cost in the prompt. Vision encoders
/// resize to a bounded patch grid, so this is a flat allowance rather than a
/// function of the image's size.
const IMAGE_TOKENS: u32 = 1600;

/// Whether Ollama evaluated so much less than we sent that it must have cut
/// the prompt. Measured against qwen3.5: `estimate_tokens` overshoots real
/// Japanese counts by ~1.5x, a cached prompt still reports its full count,
/// and a truncated one reports about half the context. Under a third of the
/// estimate is therefore truncation, with margin either way.
fn looks_truncated(prompt_tokens: u32, estimated: u32) -> bool {
    estimated >= 1024 && prompt_tokens.saturating_mul(3) < estimated
}

/// `num_predict` for an Ollama call: the requested budget, shrunk so prompt +
/// output fit inside `num_ctx`.
fn fit_output_budget(estimated_prompt: u32, max_tokens: u32, num_ctx: u32) -> Result<u32, LlmError> {
    let room = num_ctx.saturating_sub(estimated_prompt + TEMPLATE_OVERHEAD_TOKENS);
    if room < MIN_OUTPUT_TOKENS.min(max_tokens) {
        return Err(LlmError::PromptTooLarge {
            estimated_tokens: estimated_prompt,
            num_ctx,
        });
    }
    Ok(max_tokens.min(room))
}

/// Per-call overrides of the configured request settings. `Default` means
/// "use the config as-is".
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Overrides {
    /// Replaces `[ollama].think`. Ignored by the Anthropic provider.
    pub think: Option<bool>,
    /// Replaces `[llm].max_output_tokens`.
    pub max_tokens: Option<u32>,
}

/// Request features beyond plain text, used by the OCR arbitration pass.
#[derive(Debug, Default, Clone, Copy)]
pub struct Extras<'a> {
    /// PNG images attached to the user turn.
    pub images: &'a [Vec<u8>],
    /// JSON schema the reply must follow. Ollama enforces it via `format`;
    /// for Anthropic the caller should also describe it in the prompt.
    pub format: Option<&'a serde_json::Value>,
    /// Replaces `[ollama].chat_model`. Ignored by the Anthropic provider.
    pub model: Option<&'a str>,
    /// Sampling temperature; `None` leaves the provider default.
    pub temperature: Option<f32>,
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
    overrides: Overrides,
    cancel: Option<&watch::Receiver<bool>>,
) -> Result<String, LlmError> {
    complete_ext(http, cfg, system, user, overrides, Extras::default(), cancel).await
}

/// `complete` with images, a reply schema, or a different model.
pub async fn complete_ext(
    http: &reqwest::Client,
    cfg: &Config,
    system: &str,
    user: &str,
    overrides: Overrides,
    extras: Extras<'_>,
    cancel: Option<&watch::Receiver<bool>>,
) -> Result<String, LlmError> {
    let max_tokens = overrides.max_tokens.unwrap_or(cfg.llm.max_output_tokens);
    let think = overrides.think.unwrap_or(cfg.ollama.think);
    let call = || async move {
        match cfg.llm.provider {
            LlmProvider::Ollama => {
                ollama_chat(http, &cfg.ollama, system, user, think, max_tokens, extras).await
            }
            LlmProvider::Anthropic => {
                anthropic_chat(http, &cfg.anthropic, system, user, max_tokens, extras).await
            }
        }
    };
    match cancel {
        Some(rx) => {
            let mut rx = rx.clone();
            tokio::select! {
                res = call() => res,
                changed = rx.changed() => {
                    if changed.is_ok() && *rx.borrow() {
                        Err(LlmError::Cancelled)
                    } else {
                        // Watch channel closed without a cancel signal — fall
                        // through to a fresh await with no cancel.
                        call().await
                    }
                }
            }
        }
        None => call().await,
    }
}

#[derive(Serialize)]
struct OllamaChatRequest<'a> {
    model: &'a str,
    messages: Vec<OllamaChatMessage<'a>>,
    stream: bool,
    /// Omitted (rather than `false`) on the retry for models that reject the
    /// field entirely; see `ollama_chat`.
    #[serde(skip_serializing_if = "Option::is_none")]
    think: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<&'a serde_json::Value>,
    options: OllamaChatOptions,
}

#[derive(Serialize)]
struct OllamaChatMessage<'a> {
    role: &'a str,
    content: &'a str,
    /// Base64-encoded images for vision models.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    images: Vec<String>,
}

fn base64_images(images: &[Vec<u8>]) -> Vec<String> {
    images
        .iter()
        .map(|b| base64::engine::general_purpose::STANDARD.encode(b))
        .collect()
}

#[derive(Serialize)]
struct OllamaChatOptions {
    num_predict: u32,
    num_ctx: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Deserialize)]
struct OllamaChatResponse {
    message: OllamaChatResponseMessage,
    #[serde(default)]
    done_reason: Option<String>,
    /// Prompt tokens the model actually evaluated. Lower than what we sent
    /// means Ollama cut the prompt to fit its context.
    #[serde(default)]
    prompt_eval_count: Option<u32>,
}

#[derive(Deserialize)]
struct OllamaChatResponseMessage {
    content: String,
    #[serde(default)]
    thinking: Option<String>,
}

async fn ollama_chat(
    http: &reqwest::Client,
    cfg: &OllamaConfig,
    system: &str,
    user: &str,
    think: bool,
    max_tokens: u32,
    extras: Extras<'_>,
) -> Result<String, LlmError> {
    let endpoint = format!("{}/api/chat", cfg.url.trim_end_matches('/'));
    let model = extras.model.unwrap_or(&cfg.chat_model);
    let estimated_prompt = estimate_tokens(system)
        + estimate_tokens(user)
        + IMAGE_TOKENS * extras.images.len() as u32;
    let max_tokens = fit_output_budget(estimated_prompt, max_tokens, cfg.num_ctx)?;
    let mut body = OllamaChatRequest {
        model,
        messages: vec![
            OllamaChatMessage {
                role: "system",
                content: system,
                images: Vec::new(),
            },
            OllamaChatMessage {
                role: "user",
                content: user,
                images: base64_images(extras.images),
            },
        ],
        stream: false,
        think: Some(think),
        format: extras.format,
        options: OllamaChatOptions {
            num_predict: max_tokens,
            num_ctx: cfg.num_ctx,
            temperature: extras.temperature,
        },
    };

    let resp = loop {
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
        if status.is_success() {
            break resp;
        }
        let text = resp.text().await.unwrap_or_default();
        // Ollama rejects `think` (even `false`) for models without thinking
        // support. Retry once without the field so `think = true` stays a
        // safe default regardless of which chat model is configured.
        if body.think.is_some() && text.contains("does not support thinking") {
            tracing::warn!(
                model = %model,
                "chat model does not support thinking; retrying without it"
            );
            body.think = None;
            continue;
        }
        return Err(LlmError::Status {
            provider: "ollama",
            url: endpoint,
            status: status.as_u16(),
            body: text,
        });
    };

    let parsed: OllamaChatResponse = resp.json().await.map_err(|source| LlmError::Decode {
        url: endpoint,
        source,
    })?;
    if let Some(prompt_tokens) = parsed.prompt_eval_count
        && looks_truncated(prompt_tokens, estimated_prompt)
    {
        return Err(LlmError::PromptTruncated {
            model: model.to_string(),
            prompt_tokens,
            estimated_tokens: estimated_prompt,
            num_ctx: cfg.num_ctx,
        });
    }
    let content = parsed.message.content.trim().to_string();
    if content.is_empty() {
        let thought = parsed
            .message
            .thinking
            .as_deref()
            .is_some_and(|t| !t.trim().is_empty());
        if thought && parsed.done_reason.as_deref() == Some("length") {
            return Err(LlmError::ThinkingExhausted {
                provider: "ollama",
                model: model.to_string(),
                max_tokens,
            });
        }
        return Err(LlmError::EmptyResponse {
            provider: "ollama",
            model: model.to_string(),
        });
    }
    Ok(content)
}

#[derive(Serialize)]
struct AnthropicRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    system: &'a str,
    messages: Vec<AnthropicMessage<'a>>,
}

#[derive(Serialize)]
struct AnthropicMessage<'a> {
    role: &'a str,
    /// A plain string, or content blocks when images are attached.
    content: serde_json::Value,
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

fn anthropic_content(user: &str, images: &[Vec<u8>]) -> serde_json::Value {
    if images.is_empty() {
        return serde_json::Value::String(user.to_string());
    }
    let mut blocks: Vec<serde_json::Value> = base64_images(images)
        .into_iter()
        .map(|data| {
            serde_json::json!({
                "type": "image",
                "source": {"type": "base64", "media_type": "image/png", "data": data},
            })
        })
        .collect();
    blocks.push(serde_json::json!({"type": "text", "text": user}));
    serde_json::Value::Array(blocks)
}

async fn anthropic_chat(
    http: &reqwest::Client,
    cfg: &AnthropicConfig,
    system: &str,
    user: &str,
    max_tokens: u32,
    extras: Extras<'_>,
) -> Result<String, LlmError> {
    let api_key =
        std::env::var(&cfg.api_key_env).map_err(|_| LlmError::MissingApiKey {
            var: cfg.api_key_env.clone(),
        })?;
    let endpoint = format!("{}/v1/messages", cfg.base_url.trim_end_matches('/'));
    let body = AnthropicRequest {
        model: &cfg.model,
        max_tokens,
        temperature: extras.temperature,
        system,
        messages: vec![AnthropicMessage {
            role: "user",
            content: anthropic_content(user, extras.images),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_counts_cjk_per_char_and_ascii_per_four_bytes() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("abcde"), 2);
        assert_eq!(estimate_tokens("セシル"), 3);
        assert_eq!(estimate_tokens("Cecil セシル"), 2 + 3);
    }

    #[test]
    fn truncation_check_matches_measured_ollama_counts() {
        // 9,900 Japanese chars: estimated 9,904, real 6,621, and 2,050 when
        // Ollama cut it to fit a 4K context.
        assert!(!looks_truncated(6621, 9904));
        assert!(looks_truncated(2050, 9904));
        // Short prompts never trip it.
        assert!(!looks_truncated(22, 30));
    }

    #[test]
    fn output_budget_passes_through_when_it_fits() {
        assert_eq!(fit_output_budget(1000, 4000, 32768).unwrap(), 4000);
    }

    #[test]
    fn output_budget_shrinks_to_the_remaining_context() {
        let n = fit_output_budget(15_000, 20_000, 32768).unwrap();
        assert_eq!(n, 32768 - 15_000 - TEMPLATE_OVERHEAD_TOKENS);
    }

    #[test]
    fn output_budget_rejects_a_prompt_that_fills_the_context() {
        let err = fit_output_budget(32_000, 20_000, 32768).unwrap_err();
        assert!(matches!(err, LlmError::PromptTooLarge { .. }));
    }

    #[test]
    fn output_budget_allows_small_requested_budgets_near_the_limit() {
        // Asking for less than MIN_OUTPUT_TOKENS is fine as long as it fits.
        assert_eq!(fit_output_budget(32_000, 500, 32768).unwrap(), 500);
    }
}
