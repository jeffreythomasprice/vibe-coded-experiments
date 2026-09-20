use std::sync::Arc;

use crate::llm::LlmError;
use crate::llm::config::LlmConfig;
use crate::llm::provider::{ChatRequest, ChatResponse, Provider};

/// A sync front door onto the async `Provider` layer, for callers (`rewrite`,
/// `eval`) that must not themselves become async: `generate::generate` is
/// blocking FFI and cannot share a thread with an async task.
///
/// Holds one `tokio::runtime::Runtime` for the lifetime of a `generate`
/// invocation rather than building one per call. `new_current_thread` is
/// deliberate: this runs a handful of sequential HTTP requests, not concurrent
/// work, so a multi-thread runtime's worker pool would be pure overhead held
/// open across the (possibly long) native generation call.
pub struct Llm {
    runtime: tokio::runtime::Runtime,
    provider: Arc<dyn Provider>,
    default_model: Option<String>,
}

impl Llm {
    pub fn new(config: &LlmConfig) -> Result<Self, std::io::Error> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        // The provider's HTTP client is built while entered so anything it
        // registers with the runtime (timers, connection pool) belongs to it.
        let provider = {
            let _guard = runtime.enter();
            crate::llm::config::provider(config)
        };
        Ok(Self {
            runtime,
            provider,
            default_model: config.model.clone(),
        })
    }

    pub fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, LlmError> {
        self.runtime.block_on(self.provider.chat(request))
    }

    /// Wraps an already-built provider (a `ScriptedProvider` in tests) instead of
    /// the real backend, so `rewrite`/`eval` can be exercised without a live LLM.
    #[cfg(test)]
    pub(crate) fn test_with_provider(provider: Arc<dyn Provider>) -> Self {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("current-thread runtime is always buildable");
        Self {
            runtime,
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
        let llm = Llm::new(&LlmConfig::default()).unwrap();
        let err = llm.model(None, "vqa-model").unwrap_err();
        assert!(matches!(err, LlmError::MissingModel { flag: "vqa-model" }));
    }
}
