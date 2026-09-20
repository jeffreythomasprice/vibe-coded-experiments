use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;

use crate::llm::ollama::OllamaProvider;
use crate::llm::provider::Provider;

pub const DEFAULT_MODEL: &str = "qwen3:4b";
pub const DEFAULT_MAX_TURNS: usize = 8;
pub const DEFAULT_OLLAMA_HOST: &str = "localhost";
pub const DEFAULT_OLLAMA_PORT: u16 = 11434;
pub const DEFAULT_OLLAMA_TIMEOUT_SECS: u64 = 120;

/// The LLM backend to use. Ollama is the only one today; a second backend adds a
/// variant here and a matching arm in `provider()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    Ollama,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LlmConfig {
    pub backend: Backend,
    pub model: String,
    pub max_turns: usize,
    pub ollama: OllamaConfig,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            backend: Backend::Ollama,
            model: DEFAULT_MODEL.to_owned(),
            max_turns: DEFAULT_MAX_TURNS,
            ollama: OllamaConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OllamaConfig {
    pub host: String,
    pub port: u16,
    pub timeout_secs: u64,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            host: DEFAULT_OLLAMA_HOST.to_owned(),
            port: DEFAULT_OLLAMA_PORT,
            timeout_secs: DEFAULT_OLLAMA_TIMEOUT_SECS,
        }
    }
}

impl OllamaConfig {
    pub fn base_url(&self) -> String {
        format!("http://{}:{}", self.host, self.port)
    }

    pub fn timeout(&self) -> Duration {
        Duration::from_secs(self.timeout_secs)
    }
}

/// Builds the configured backend's `Provider`. The only backend today is Ollama;
/// this is the extension point a second `Backend` variant plugs into.
pub fn provider(config: &LlmConfig) -> Arc<dyn Provider> {
    match config.backend {
        Backend::Ollama => Arc::new(OllamaProvider::new(
            config.ollama.base_url(),
            config.ollama.timeout(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_point_at_localhost() {
        let config = LlmConfig::default();
        assert_eq!(config.backend, Backend::Ollama);
        assert_eq!(config.ollama.host, "localhost");
        assert_eq!(config.ollama.port, 11434);
        assert_eq!(config.ollama.base_url(), "http://localhost:11434");
    }

    #[test]
    fn toml_overrides_are_applied() {
        let parsed: LlmConfig = toml::from_str(
            "model = \"llama3.2\"\nmax_turns = 3\n[ollama]\nhost = \"10.0.0.5\"\nport = 9999\n",
        )
        .unwrap();
        assert_eq!(parsed.model, "llama3.2");
        assert_eq!(parsed.max_turns, 3);
        assert_eq!(parsed.ollama.base_url(), "http://10.0.0.5:9999");
    }

    #[test]
    fn unknown_top_level_key_is_rejected() {
        let err = toml::from_str::<LlmConfig>("not_a_real_key = 1\n").unwrap_err();
        assert!(err.to_string().contains("unknown field"));
    }

    #[test]
    fn unknown_ollama_key_is_rejected() {
        let err = toml::from_str::<LlmConfig>("[ollama]\nnope = true\n").unwrap_err();
        assert!(err.to_string().contains("unknown field"));
    }

    #[test]
    fn unknown_backend_value_is_rejected() {
        let err = toml::from_str::<LlmConfig>("backend = \"claude\"\n").unwrap_err();
        assert!(err.to_string().contains("ollama"));
    }

    #[test]
    fn partial_file_keeps_other_defaults() {
        let parsed: LlmConfig = toml::from_str("model = \"llama3.2\"\n").unwrap();
        assert_eq!(parsed.max_turns, DEFAULT_MAX_TURNS);
        assert_eq!(parsed.ollama.port, DEFAULT_OLLAMA_PORT);
    }
}
