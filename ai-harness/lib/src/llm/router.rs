//! A generic representation of "a provider": a name plus whichever of the
//! four `lib::llm` capability traits it actually implements, addressable
//! through one [`Router`].
//!
//! This is exactly the capability matrix `AnthropicClient`/`OllamaClient`/
//! `OpenAiClient` already establish — Anthropic: chat + models; Ollama: chat +
//! models + embeddings; OpenAI: all four — made dynamic without giving up
//! "capability discovery is by type, not a runtime flag": a [`ProviderEntry`]
//! still only exposes the capabilities it was actually built with, and a
//! request for one it lacks is a [`LlmError::Unsupported`], not a silently
//! absent method.
//!
//! There is deliberately no bare-provider-name or bare-model-id lookup — see
//! [`shared::llm::ModelRef`]'s doc for why. Every lookup here takes one.

use std::collections::BTreeMap;
use std::sync::Arc;

use shared::llm::ModelRef;

use crate::llm::config::LlmConfig;
use crate::llm::error::LlmError;
use crate::llm::{
    AnthropicClient, ChatProvider, EmbeddingProvider, ImageProvider, ModelProvider, OllamaClient,
    OpenAiClient,
};

/// One provider's registered capabilities. Any field left `None` simply isn't
/// offered — a gateway that only proxies chat need not (and, if it can't,
/// must not) also claim `models`/`embeddings`/`images`.
#[derive(Default, Clone)]
pub struct ProviderEntry {
    pub chat: Option<Arc<dyn ChatProvider>>,
    pub models: Option<Arc<dyn ModelProvider>>,
    pub embeddings: Option<Arc<dyn EmbeddingProvider>>,
    pub images: Option<Arc<dyn ImageProvider>>,
}

/// Resolves a [`ModelRef`] to a live provider client. The map key is
/// independent of any client's own `ChatProvider::name()` — an
/// OpenAI-compatible gateway can register under `"my-gateway"` while the
/// client underneath still reports `"openai"` in its own error messages.
#[derive(Default)]
pub struct Router {
    entries: BTreeMap<String, ProviderEntry>,
}

impl Router {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers `anthropic`, `ollama`, and `openai` from `[llm]`, each with
    /// the capabilities its client actually implements. A provider whose
    /// client fails to build (today, only a `reqwest::Client` construction
    /// failure — see `crate::llm::http::build_client`) is logged and simply
    /// left unregistered, the same "one bad provider doesn't blank out the
    /// others" policy `crate::catalog::load` already follows; a missing API
    /// key is *not* such a failure; it's read from the environment lazily, at
    /// request time, so a provider you never call is registered without one.
    pub fn from_config(config: &LlmConfig) -> Self {
        let mut router = Self::new();
        let timeout = config.request_timeout();
        let max_retries = config.max_retries;

        match AnthropicClient::new(config.anthropic.clone(), timeout, max_retries) {
            Ok(client) => {
                let client = Arc::new(client);
                router.register(
                    "anthropic",
                    ProviderEntry {
                        chat: Some(client.clone()),
                        models: Some(client),
                        ..Default::default()
                    },
                );
            }
            Err(err) => tracing::warn!(provider = "anthropic", %err, "failed to build client"),
        }

        match OllamaClient::new(config.ollama.clone(), timeout, max_retries) {
            Ok(client) => {
                let client = Arc::new(client);
                router.register(
                    "ollama",
                    ProviderEntry {
                        chat: Some(client.clone()),
                        models: Some(client.clone()),
                        embeddings: Some(client),
                        images: None,
                    },
                );
            }
            Err(err) => tracing::warn!(provider = "ollama", %err, "failed to build client"),
        }

        match OpenAiClient::new(config.openai.clone(), timeout, max_retries) {
            Ok(client) => {
                let client = Arc::new(client);
                router.register(
                    "openai",
                    ProviderEntry {
                        chat: Some(client.clone()),
                        models: Some(client.clone()),
                        embeddings: Some(client.clone()),
                        images: Some(client),
                    },
                );
            }
            Err(err) => tracing::warn!(provider = "openai", %err, "failed to build client"),
        }

        router
    }

    /// Register an arbitrary provider under `name` — a gateway, a proxy, or
    /// (in tests) a scripted double. Overwrites whatever was registered under
    /// the same name before.
    pub fn register(&mut self, name: impl Into<String>, entry: ProviderEntry) {
        self.entries.insert(name.into(), entry);
    }

    pub fn chat(&self, model: &ModelRef) -> Result<Arc<dyn ChatProvider>, LlmError> {
        self.entry(&model.provider)?
            .chat
            .clone()
            .ok_or_else(|| unsupported(&model.provider, "chat"))
    }

    pub fn models(&self, model: &ModelRef) -> Result<Arc<dyn ModelProvider>, LlmError> {
        self.entry(&model.provider)?
            .models
            .clone()
            .ok_or_else(|| unsupported(&model.provider, "model listing"))
    }

    pub fn embeddings(&self, model: &ModelRef) -> Result<Arc<dyn EmbeddingProvider>, LlmError> {
        self.entry(&model.provider)?
            .embeddings
            .clone()
            .ok_or_else(|| unsupported(&model.provider, "embeddings"))
    }

    pub fn images(&self, model: &ModelRef) -> Result<Arc<dyn ImageProvider>, LlmError> {
        self.entry(&model.provider)?
            .images
            .clone()
            .ok_or_else(|| unsupported(&model.provider, "image generation"))
    }

    /// Every registered provider name, in the router's iteration order.
    pub fn provider_names(&self) -> Vec<&str> {
        self.entries.keys().map(String::as_str).collect()
    }

    fn entry(&self, provider: &str) -> Result<&ProviderEntry, LlmError> {
        self.entries
            .get(provider)
            .ok_or_else(|| LlmError::UnknownProvider {
                name: provider.to_string(),
                available: self
                    .provider_names()
                    .into_iter()
                    .map(String::from)
                    .collect(),
            })
    }
}

fn unsupported(provider: &str, capability: &'static str) -> LlmError {
    LlmError::Unsupported {
        provider: provider.to_string(),
        capability,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::testing::ScriptedProvider;

    fn router_with_one_chat_provider(name: &'static str) -> Router {
        let mut router = Router::new();
        router.register(
            name,
            ProviderEntry {
                chat: Some(Arc::new(ScriptedProvider::new(vec![]).named(name))),
                ..Default::default()
            },
        );
        router
    }

    /// `Arc<dyn ChatProvider>` isn't `Debug`, so `unwrap_err()` (which needs
    /// `T: Debug` to format a panic message on the `Ok` path) can't be used on
    /// a `Result<Arc<dyn ChatProvider>, LlmError>` — this pulls the error out
    /// without that bound.
    fn expect_err(result: Result<Arc<dyn ChatProvider>, LlmError>) -> LlmError {
        match result {
            Err(err) => err,
            Ok(_) => panic!("expected an error"),
        }
    }

    #[test]
    fn unknown_provider_names_every_registered_one() {
        let mut router = router_with_one_chat_provider("anthropic");
        router.register("ollama", ProviderEntry::default());

        let err = expect_err(router.chat(&ModelRef::new("azure", "gpt-5.6")));
        match err {
            LlmError::UnknownProvider { name, available } => {
                assert_eq!(name, "azure");
                assert_eq!(
                    available,
                    vec!["anthropic".to_string(), "ollama".to_string()]
                );
            }
            other => panic!("expected UnknownProvider, got {other:?}"),
        }
    }

    #[test]
    fn a_registered_provider_without_a_capability_reports_unsupported() {
        let mut router = Router::new();
        router.register("anthropic", ProviderEntry::default());

        let err = expect_err(router.chat(&ModelRef::new("anthropic", "claude-opus-5")));
        match err {
            LlmError::Unsupported {
                provider,
                capability,
            } => {
                assert_eq!(provider, "anthropic");
                assert_eq!(capability, "chat");
            }
            other => panic!("expected Unsupported, got {other:?}"),
        }
    }

    #[test]
    fn a_custom_registration_resolves_by_its_registered_name() {
        let router = router_with_one_chat_provider("my-gateway");
        let provider = router
            .chat(&ModelRef::new("my-gateway", "whatever-model"))
            .unwrap();
        assert_eq!(provider.name(), "my-gateway");
    }

    #[test]
    fn registering_the_same_name_twice_replaces_the_entry() {
        let mut router = router_with_one_chat_provider("gw");
        router.register("gw", ProviderEntry::default());
        let err = expect_err(router.chat(&ModelRef::new("gw", "m")));
        assert!(matches!(err, LlmError::Unsupported { .. }));
    }

    #[test]
    fn from_config_registers_all_three_providers_with_their_real_capability_sets() {
        let router = Router::from_config(&LlmConfig::default());
        assert_eq!(
            router.provider_names(),
            vec!["anthropic", "ollama", "openai"]
        );

        // Anthropic: chat + models, no embeddings/images.
        assert!(router.chat(&ModelRef::new("anthropic", "m")).is_ok());
        assert!(router.models(&ModelRef::new("anthropic", "m")).is_ok());
        assert!(router.embeddings(&ModelRef::new("anthropic", "m")).is_err());
        assert!(router.images(&ModelRef::new("anthropic", "m")).is_err());

        // Ollama: chat + models + embeddings, no images.
        assert!(router.chat(&ModelRef::new("ollama", "m")).is_ok());
        assert!(router.embeddings(&ModelRef::new("ollama", "m")).is_ok());
        assert!(router.images(&ModelRef::new("ollama", "m")).is_err());

        // OpenAI: all four.
        assert!(router.chat(&ModelRef::new("openai", "m")).is_ok());
        assert!(router.models(&ModelRef::new("openai", "m")).is_ok());
        assert!(router.embeddings(&ModelRef::new("openai", "m")).is_ok());
        assert!(router.images(&ModelRef::new("openai", "m")).is_ok());
    }
}
