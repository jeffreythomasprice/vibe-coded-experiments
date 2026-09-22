use std::sync::Arc;

use crate::llm::LlmError;
use crate::llm::config::LlmConfig;
use crate::llm::provider::{ChatRequest, ChatResponse, Provider};

/// The async front door onto the `Provider` layer, for callers (`rewrite`, `eval`).
pub struct Llm {
    provider: Arc<dyn Provider>,
    default_model: Option<String>,
}

impl Llm {
    pub fn new(config: &LlmConfig) -> Self {
        Self {
            provider: crate::llm::config::provider(config),
            default_model: config.model.clone(),
        }
    }

    pub async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, LlmError> {
        self.provider.chat(request).await
    }

    /// Wraps an already-built provider (a `ScriptedProvider` in tests) instead of
    /// the real backend, so `rewrite`/`eval` can be exercised without a live LLM.
    #[cfg(test)]
    pub(crate) fn test_with_provider(provider: Arc<dyn Provider>) -> Self {
        Self {
            provider,
            default_model: Some("test-model".to_owned()),
        }
    }

    /// `models` are per-role overrides (`--vqa-model`, `--caption-model`, ...);
    /// an unset override falls back to `[llm].model`. There is no further,
    /// built-in default, so neither set is `MissingModel`.
    pub fn model(&self, override_model: Option<&str>, flag: &'static str) -> Result<String, LlmError> {
        override_model
            .map(str::to_owned)
            .or_else(|| self.default_model.clone())
            .ok_or(LlmError::MissingModel { flag })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_falls_back_to_configured_default() {
        let llm = Llm::test_with_provider(Arc::new(crate::llm::mock::ScriptedProvider::new(vec![])));
        assert_eq!(llm.model(None, "vqa-model").unwrap(), "test-model");
        assert_eq!(llm.model(Some("qwen3.8:latest"), "vqa-model").unwrap(), "qwen3.8:latest");
    }

    #[test]
    fn model_fails_without_override_or_configured_default() {
        let llm = Llm::new(&LlmConfig::default());
        let err = llm.model(None, "vqa-model").unwrap_err();
        assert!(matches!(err, LlmError::MissingModel { flag: "vqa-model" }));
    }
}
