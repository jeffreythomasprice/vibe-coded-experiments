//! `[llm]` configuration: shared HTTP settings plus one table per provider.
//!
//! Mirrors the pattern in `crate::config`: every struct is
//! `#[serde(deny_unknown_fields)]`, and every field has a
//! `#[serde(default = "…")]` backed by the same function the hand-written
//! `Default` impl uses — so a missing table, an empty one, and one that
//! spells out the defaults all behave identically.

use serde::{Deserialize, Serialize};

use crate::llm::error::LlmError;

/// The `[llm]` table.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LlmConfig {
    /// Per-request timeout, in seconds unless suffixed. Accepts a bare
    /// integer or a suffixed string — `"120s"`, `"2m"`, `"1h"`.
    #[serde(
        default = "default_request_timeout_secs",
        deserialize_with = "deserialize_duration_secs"
    )]
    pub request_timeout_secs: u64,
    /// How many times to retry a retryable failure (429, 5xx, 529) before
    /// giving up. Zero disables retries.
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default)]
    pub anthropic: AnthropicConfig,
    #[serde(default)]
    pub ollama: OllamaConfig,
    #[serde(default)]
    pub openai: OpenAiConfig,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            request_timeout_secs: default_request_timeout_secs(),
            max_retries: default_max_retries(),
            anthropic: AnthropicConfig::default(),
            ollama: OllamaConfig::default(),
            openai: OpenAiConfig::default(),
        }
    }
}

impl LlmConfig {
    pub fn request_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.request_timeout_secs)
    }
}

fn default_request_timeout_secs() -> u64 {
    120
}

fn default_max_retries() -> u32 {
    2
}

/// The `[llm.anthropic]` table.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AnthropicConfig {
    /// Scheme, host, and port — e.g. `http://localhost:11434` for a proxy.
    /// There is no separate `port` key: a URL that carries its own port is
    /// unambiguous, where a split `host`/`port` pair could disagree with it.
    #[serde(default = "default_anthropic_base_url")]
    pub base_url: String,
    /// Name of the environment variable holding the API key — never the key
    /// itself. The effective config is dumped to the log on every startup
    /// (`server/src/main.rs`), so storing the value here would leak it.
    #[serde(default = "default_anthropic_api_key_env")]
    pub api_key_env: String,
    #[serde(default = "default_anthropic_model")]
    pub model: String,
    #[serde(default = "default_anthropic_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_anthropic_version")]
    pub anthropic_version: String,
}

impl Default for AnthropicConfig {
    fn default() -> Self {
        Self {
            base_url: default_anthropic_base_url(),
            api_key_env: default_anthropic_api_key_env(),
            model: default_anthropic_model(),
            max_tokens: default_anthropic_max_tokens(),
            anthropic_version: default_anthropic_version(),
        }
    }
}

impl AnthropicConfig {
    /// Read the API key from the environment variable named in
    /// `api_key_env` — never from the config file itself.
    pub fn api_key(&self) -> Result<String, LlmError> {
        std::env::var(&self.api_key_env).map_err(|_| LlmError::MissingApiKey {
            var: self.api_key_env.clone(),
        })
    }
}

fn default_anthropic_base_url() -> String {
    "https://api.anthropic.com".to_string()
}

fn default_anthropic_api_key_env() -> String {
    "ANTHROPIC_API_KEY".to_string()
}

fn default_anthropic_model() -> String {
    "claude-opus-5".to_string()
}

fn default_anthropic_max_tokens() -> u32 {
    16_000
}

fn default_anthropic_version() -> String {
    "2023-06-01".to_string()
}

/// The `[llm.ollama]` table.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OllamaConfig {
    #[serde(default = "default_ollama_base_url")]
    pub base_url: String,
    #[serde(default = "default_ollama_model")]
    pub model: String,
    /// How long Ollama keeps the model loaded after this request. Passed
    /// through verbatim as Ollama's own duration string (e.g. `"5m"`).
    #[serde(default = "default_ollama_keep_alive")]
    pub keep_alive: String,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            base_url: default_ollama_base_url(),
            model: default_ollama_model(),
            keep_alive: default_ollama_keep_alive(),
        }
    }
}

fn default_ollama_base_url() -> String {
    "http://localhost:11434".to_string()
}

fn default_ollama_model() -> String {
    "qwen3.5:latest".to_string()
}

fn default_ollama_keep_alive() -> String {
    "5m".to_string()
}

/// The `[llm.openai]` table.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OpenAiConfig {
    #[serde(default = "default_openai_base_url")]
    pub base_url: String,
    #[serde(default = "default_openai_api_key_env")]
    pub api_key_env: String,
    #[serde(default = "default_openai_model")]
    pub model: String,
    /// Model used for `ImageProvider::generate` — a distinct chat vs. image
    /// model, since OpenAI's chat and image-generation models are disjoint.
    #[serde(default = "default_openai_image_model")]
    pub image_model: String,
}

impl Default for OpenAiConfig {
    fn default() -> Self {
        Self {
            base_url: default_openai_base_url(),
            api_key_env: default_openai_api_key_env(),
            model: default_openai_model(),
            image_model: default_openai_image_model(),
        }
    }
}

impl OpenAiConfig {
    pub fn api_key(&self) -> Result<String, LlmError> {
        std::env::var(&self.api_key_env).map_err(|_| LlmError::MissingApiKey {
            var: self.api_key_env.clone(),
        })
    }
}

fn default_openai_base_url() -> String {
    "https://api.openai.com".to_string()
}

fn default_openai_api_key_env() -> String {
    "OPENAI_API_KEY".to_string()
}

fn default_openai_model() -> String {
    "gpt-5.6".to_string()
}

fn default_openai_image_model() -> String {
    "gpt-image-2".to_string()
}

/// Accepts either a TOML integer (seconds) or a suffixed string.
#[derive(Deserialize)]
#[serde(untagged)]
enum DurationSpec {
    Seconds(u64),
    Text(String),
}

fn deserialize_duration_secs<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    match DurationSpec::deserialize(deserializer)? {
        DurationSpec::Seconds(n) => Ok(n),
        DurationSpec::Text(s) => parse_duration_secs(&s).map_err(serde::de::Error::custom),
    }
}

/// Parse a duration: a bare number (seconds), or a number with an `s`/`m`/`h`
/// suffix. Case-insensitive; whitespace between number and suffix is
/// permitted. Mirrors `crate::config::parse_size`.
fn parse_duration_secs(s: &str) -> Result<u64, String> {
    let t = s.trim();
    let split = t.find(|c: char| !c.is_ascii_digit()).unwrap_or(t.len());
    let (digits, suffix) = t.split_at(split);
    if digits.is_empty() {
        return Err(format!("invalid duration {s:?}: expected a leading number"));
    }
    let n: u64 = digits
        .parse()
        .map_err(|_| format!("invalid duration {s:?}: {digits:?} is not a number"))?;

    let multiplier = match suffix.trim().to_ascii_lowercase().as_str() {
        "" | "s" => 1,
        "m" => 60,
        "h" => 3_600,
        other => return Err(format!("invalid duration {s:?}: unknown unit {other:?}")),
    };
    n.checked_mul(multiplier)
        .ok_or_else(|| format!("invalid duration {s:?}: overflows u64"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_durations_with_and_without_units() {
        assert_eq!(parse_duration_secs("120"), Ok(120));
        assert_eq!(parse_duration_secs("120s"), Ok(120));
        assert_eq!(parse_duration_secs("2m"), Ok(120));
        assert_eq!(parse_duration_secs("1h"), Ok(3_600));
        assert_eq!(parse_duration_secs("1 h"), Ok(3_600));
        assert_eq!(parse_duration_secs("2M"), Ok(120));
    }

    #[test]
    fn rejects_garbage_durations() {
        assert!(parse_duration_secs("").is_err());
        assert!(parse_duration_secs("m").is_err());
        assert!(parse_duration_secs("100 lightyears").is_err());
        assert!(parse_duration_secs("18446744073709551615h").is_err()); // overflow
    }

    #[test]
    fn empty_config_matches_builtin_provider_defaults() {
        let parsed: LlmConfig = toml::from_str("").unwrap();
        let defaults = LlmConfig::default();
        assert_eq!(parsed.request_timeout_secs, defaults.request_timeout_secs);
        assert_eq!(parsed.max_retries, defaults.max_retries);
        assert_eq!(parsed.anthropic.base_url, defaults.anthropic.base_url);
        assert_eq!(parsed.anthropic.model, defaults.anthropic.model);
        assert_eq!(parsed.ollama.base_url, defaults.ollama.base_url);
        assert_eq!(parsed.openai.base_url, defaults.openai.base_url);
        assert_eq!(parsed.openai.model, defaults.openai.model);
    }

    #[test]
    fn parses_a_full_llm_config() {
        let parsed: LlmConfig = toml::from_str(
            r#"
            request_timeout_secs = "5m"
            max_retries = 5
            [anthropic]
            base_url = "http://localhost:9999"
            api_key_env = "MY_KEY"
            model = "claude-haiku-4-5"
            max_tokens = 1024
            anthropic_version = "2023-06-01"
            [ollama]
            base_url = "http://localhost:11434"
            model = "llama3.1:8b"
            keep_alive = "1m"
            [openai]
            base_url = "https://api.openai.com"
            api_key_env = "MY_OPENAI_KEY"
            model = "gpt-5.6"
            image_model = "gpt-image-1-mini"
            "#,
        )
        .unwrap();
        assert_eq!(parsed.request_timeout_secs, 300);
        assert_eq!(parsed.max_retries, 5);
        assert_eq!(parsed.anthropic.model, "claude-haiku-4-5");
        assert_eq!(parsed.ollama.model, "llama3.1:8b");
        assert_eq!(parsed.openai.image_model, "gpt-image-1-mini");
    }

    #[test]
    fn rejects_unknown_provider_keys() {
        assert!(toml::from_str::<LlmConfig>("[anthropic]\nnope = 1\n").is_err());
        assert!(toml::from_str::<LlmConfig>("[ollama]\nport = 1\n").is_err());
        assert!(toml::from_str::<LlmConfig>("nope = 1\n").is_err());
    }

    #[test]
    fn api_key_is_read_from_the_named_env_var_not_the_config() {
        let cfg = AnthropicConfig {
            api_key_env: "AI_HARNESS_TEST_API_KEY_VAR".to_string(),
            ..AnthropicConfig::default()
        };
        // SAFETY: this test only reads/writes an env var it owns exclusively
        // (a name unlikely to collide) and cleans up before returning.
        unsafe {
            std::env::set_var("AI_HARNESS_TEST_API_KEY_VAR", "sk-test-123");
        }
        let key = cfg.api_key().unwrap();
        assert_eq!(key, "sk-test-123");
        unsafe {
            std::env::remove_var("AI_HARNESS_TEST_API_KEY_VAR");
        }
    }

    #[test]
    fn missing_api_key_env_var_names_the_variable() {
        let cfg = AnthropicConfig {
            api_key_env: "AI_HARNESS_TEST_DEFINITELY_UNSET_VAR".to_string(),
            ..AnthropicConfig::default()
        };
        unsafe {
            std::env::remove_var("AI_HARNESS_TEST_DEFINITELY_UNSET_VAR");
        }
        let err = cfg.api_key().unwrap_err();
        assert!(matches!(
            &err,
            LlmError::MissingApiKey { var } if var == "AI_HARNESS_TEST_DEFINITELY_UNSET_VAR"
        ));
        assert!(err.to_string().contains("AI_HARNESS_TEST_DEFINITELY_UNSET_VAR"));
    }
}
