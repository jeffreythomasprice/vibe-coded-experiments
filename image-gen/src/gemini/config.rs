use std::time::Duration;

use serde::Deserialize;

use crate::gemini::GeminiError;

pub const DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta";
pub const DEFAULT_TIMEOUT_SECS: u64 = 300;
pub const API_KEY_ENV: &str = "GEMINI_API_KEY";
pub const DEFAULT_IMAGE_MODEL: &str = "gemini-2.5-flash-image";

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GeminiImageConfig {
    pub model: String,
    /// "1K", "2K" or "4K"; only accepted by models that support it
    /// (e.g. gemini-3-pro-image-preview).
    pub image_size: Option<String>,
}

impl Default for GeminiImageConfig {
    fn default() -> Self {
        Self {
            model: DEFAULT_IMAGE_MODEL.to_owned(),
            image_size: None,
        }
    }
}

#[derive(Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GeminiConfig {
    /// Falls back to the `GEMINI_API_KEY` environment variable when unset.
    pub api_key: Option<String>,
    pub base_url: String,
    pub timeout_secs: u64,
}

impl std::fmt::Debug for GeminiConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GeminiConfig")
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .field("base_url", &self.base_url)
            .field("timeout_secs", &self.timeout_secs)
            .finish()
    }
}

impl Default for GeminiConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            base_url: DEFAULT_BASE_URL.to_owned(),
            timeout_secs: DEFAULT_TIMEOUT_SECS,
        }
    }
}

impl GeminiConfig {
    pub fn timeout(&self) -> Duration {
        Duration::from_secs(self.timeout_secs)
    }

    pub fn resolve_api_key(&self) -> Result<String, GeminiError> {
        self.resolve_api_key_with(std::env::var(API_KEY_ENV).ok())
    }

    fn resolve_api_key_with(&self, env: Option<String>) -> Result<String, GeminiError> {
        self.api_key
            .clone()
            .filter(|key| !key.is_empty())
            .or_else(|| env.filter(|key| !key.is_empty()))
            .ok_or(GeminiError::MissingApiKey)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let config = GeminiConfig::default();
        assert_eq!(config.api_key, None);
        assert_eq!(config.base_url, DEFAULT_BASE_URL);
        assert_eq!(config.timeout(), Duration::from_secs(300));
    }

    #[test]
    fn toml_overrides_are_applied() {
        let parsed: GeminiConfig =
            toml::from_str("api_key = \"k\"\nbase_url = \"http://x\"\ntimeout_secs = 5\n").unwrap();
        assert_eq!(parsed.api_key, Some("k".to_owned()));
        assert_eq!(parsed.base_url, "http://x");
        assert_eq!(parsed.timeout_secs, 5);
    }

    #[test]
    fn unknown_key_is_rejected() {
        let err = toml::from_str::<GeminiConfig>("nope = 1\n").unwrap_err();
        assert!(err.to_string().contains("unknown field"));
    }

    #[test]
    fn config_key_wins_over_env() {
        let config = GeminiConfig {
            api_key: Some("from-config".to_owned()),
            ..Default::default()
        };
        assert_eq!(
            config.resolve_api_key_with(Some("from-env".to_owned())).unwrap(),
            "from-config"
        );
    }

    #[test]
    fn env_key_is_the_fallback() {
        let config = GeminiConfig::default();
        assert_eq!(
            config.resolve_api_key_with(Some("from-env".to_owned())).unwrap(),
            "from-env"
        );
    }

    #[test]
    fn no_key_anywhere_is_an_error() {
        let config = GeminiConfig::default();
        assert!(matches!(
            config.resolve_api_key_with(None),
            Err(GeminiError::MissingApiKey)
        ));
        assert!(matches!(
            config.resolve_api_key_with(Some(String::new())),
            Err(GeminiError::MissingApiKey)
        ));
    }

    #[test]
    fn debug_output_redacts_the_key() {
        let config = GeminiConfig {
            api_key: Some("super-secret".to_owned()),
            ..Default::default()
        };
        assert!(!format!("{config:?}").contains("super-secret"));
    }
}
