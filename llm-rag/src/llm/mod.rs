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

pub fn build(cfg: &Config) -> Result<LlmStack, LlmError> {
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

    let embeddings: Arc<dyn EmbeddingProvider> = match &cfg.llm.embeddings {
        EmbeddingsProviderConfig::Ollama {
            url,
            model,
            dimensions,
        } => Arc::new(ollama::OllamaProvider::new_embeddings(
            url,
            model,
            *dimensions,
        )?),
    };

    Ok(LlmStack { chat, embeddings })
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
                    dimensions: 768,
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
        match build(&cfg) {
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
        build(&cfg).expect("ollama build should not need a secret");
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
        build(&cfg).expect("anthropic build with secret should succeed");
    }
}
