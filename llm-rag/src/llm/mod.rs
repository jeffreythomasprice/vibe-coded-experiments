//! Provider-agnostic LLM interface. Providers live in submodules; the public
//! entry point is [`build`] which takes a fully-loaded [`crate::config::Config`]
//! and returns a [`LlmStack`] holding trait objects for chat and embeddings.
//!
//! Most items are unused by the binary today; future RAG/chat subcommands will exercise them.
#![allow(dead_code)]

use std::sync::Arc;

use crate::config::{ChatProviderConfig, Config, EmbeddingsProviderConfig};

pub mod anthropic;
mod convert;
pub mod error;
pub mod ollama;
pub mod provider;
pub mod types;

#[cfg(any(test, feature = "mock-llm"))]
pub mod mock;

pub use error::LlmError;
#[allow(unused_imports)]
pub use provider::{EmbeddingProvider, LlmProvider};
#[allow(unused_imports)]
pub use types::{ChatRequest, ChatResponse, Message, StreamChunk, Tool, ToolCall};

pub struct LlmStack {
    pub chat: Arc<dyn LlmProvider>,
    pub embeddings: Arc<dyn EmbeddingProvider>,
}

/// Build the chat-side provider from config. Embeddings are built separately
/// via [`build_embeddings`] because their construction requires the model's
/// vector length, which is resolved from the DB cache (or probed) at startup.
pub fn build_chat(cfg: &Config) -> Result<Arc<dyn LlmProvider>, LlmError> {
    let chat: Arc<dyn LlmProvider> = match &cfg.llm.chat {
        ChatProviderConfig::Ollama { url, model } => {
            Arc::new(ollama::OllamaProvider::new(url, model)?)
        }
        ChatProviderConfig::Anthropic { model } => {
            let key = cfg
                .secrets
                .anthropic_api_key
                .as_ref()
                .ok_or(LlmError::MissingSecret("anthropic_api_key"))?;
            Arc::new(anthropic::AnthropicProvider::new(key, model)?)
        }
    };
    Ok(chat)
}

/// Build the embeddings provider once `dimensions` is known (resolved by the
/// DB layer's cache lookup / probe).
pub fn build_embeddings(
    cfg: &Config,
    dimensions: usize,
) -> Result<Arc<dyn EmbeddingProvider>, LlmError> {
    let embeddings: Arc<dyn EmbeddingProvider> = match &cfg.llm.embeddings {
        EmbeddingsProviderConfig::Ollama { url, model } => Arc::new(
            ollama::OllamaProvider::new_embeddings(url, model, dimensions)?,
        ),
    };
    Ok(embeddings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ChatProviderConfig, EmbeddingsProviderConfig, LlmConfig, SecretsConfig};
    use secrecy::SecretString;
    use std::path::PathBuf;

    fn base_config(chat: ChatProviderConfig, secrets: SecretsConfig) -> Config {
        Config {
            server_idle_timeout_secs: 10,
            client_request_timeout_secs: 30,
            socket_dir: PathBuf::from("/tmp/x"),
            log_dir: PathBuf::from("/tmp/x"),
            db_path: PathBuf::from("/tmp/x/local.db"),
            llm: LlmConfig {
                chat,
                embeddings: EmbeddingsProviderConfig::Ollama {
                    url: "http://localhost:11434".into(),
                    model: "nomic-embed-text".into(),
                },
            },
            secrets,
        }
    }

    #[test]
    fn build_anthropic_without_secret_returns_missing_secret() {
        let cfg = base_config(
            ChatProviderConfig::Anthropic {
                model: "claude".into(),
            },
            SecretsConfig::default(),
        );
        match build_chat(&cfg) {
            Err(LlmError::MissingSecret("anthropic_api_key")) => {}
            Err(other) => panic!("expected MissingSecret, got {other:?}"),
            Ok(_) => panic!("expected MissingSecret, got Ok"),
        }
    }

    #[test]
    fn build_ollama_chat_succeeds_without_secret() {
        let cfg = base_config(
            ChatProviderConfig::Ollama {
                url: "http://localhost:11434".into(),
                model: "llama3.2:3b".into(),
            },
            SecretsConfig::default(),
        );
        build_chat(&cfg).expect("ollama build should not need a secret");
        build_embeddings(&cfg, 768).expect("embeddings build is dimension-only");
    }

    #[test]
    fn build_anthropic_with_secret_succeeds() {
        let cfg = base_config(
            ChatProviderConfig::Anthropic {
                model: "claude".into(),
            },
            SecretsConfig {
                anthropic_api_key: Some(SecretString::from("sk-test")),
                loaded_from_insecure_path: None,
            },
        );
        build_chat(&cfg).expect("anthropic build with secret should succeed");
    }
}
