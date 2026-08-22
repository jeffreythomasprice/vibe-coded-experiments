//! The one deliberate deviation from this workspace's `anyhow`-everywhere
//! error style.
//!
//! `anyhow::Error` can't be matched on, and the retry loop in `http.rs` (and
//! any future router) needs to distinguish a retryable failure — 429, 5xx,
//! 529 — from a non-retryable one — 400, 401, 403. `?` still lifts an
//! `LlmError` into `anyhow::Error` for free at the `server` boundary, so
//! nothing outside `lib::llm` has to know this type exists.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum LlmError {
    #[error("{provider}: request failed: {source}")]
    Http {
        provider: &'static str,
        #[source]
        source: reqwest::Error,
    },

    #[error("{provider}: HTTP {status}: {message}")]
    Status {
        provider: &'static str,
        status: u16,
        kind: ApiErrorKind,
        message: String,
        request_id: Option<String>,
    },

    #[error("{provider}: failed to decode {context}: {source}")]
    Decode {
        provider: &'static str,
        context: String,
        #[source]
        source: serde_json::Error,
    },

    /// A hand-rolled parser (not `serde_json`) couldn't make sense of what it
    /// was given — currently only Ollama's `ollama.com/library` HTML scrape.
    /// Distinct from [`LlmError::Decode`], which is specifically a
    /// `serde_json::Error`.
    #[error("{provider}: failed to parse {context}: {message}")]
    Parse {
        provider: &'static str,
        context: String,
        message: String,
    },

    #[error("missing API key: environment variable {var} is not set")]
    MissingApiKey { var: String },

    #[error("{provider}: stream error: {message}")]
    Stream {
        provider: &'static str,
        message: String,
    },

    /// An embeddings request's input was too large for the model — a
    /// provider's own 400/413 rejection, reclassified from a plain
    /// [`LlmError::Status`] by matching the error message's wording (see
    /// each provider's `embeddings` module). Never raised speculatively:
    /// this crate never estimates a request's size and refuses it locally,
    /// only reclassifies what the API already rejected. See
    /// `shared::llm::ModelDetails::probably_fits` for an opt-in, purely
    /// advisory pre-flight estimate a caller may use instead.
    #[error("{provider}: input too large for {model}: {message}")]
    InputTooLarge {
        provider: &'static str,
        model: String,
        /// The model's limit in tokens, when the provider's model listing
        /// reports one (only `ModelDetails::OllamaLocalEmbedding` does).
        max_input_tokens: Option<u64>,
        message: String,
    },
}

impl LlmError {
    /// Whether retrying the same request might succeed. Config errors,
    /// decode errors, and 4xx statuses (other than 429) are not — retrying
    /// them would just fail the same way again.
    pub fn is_retryable(&self) -> bool {
        match self {
            LlmError::Status { kind, .. } => kind.is_retryable(),
            // A connection-level failure (DNS, TCP reset, TLS handshake) is
            // usually transient and worth one more attempt.
            LlmError::Http { .. } => true,
            LlmError::Decode { .. }
            | LlmError::Parse { .. }
            | LlmError::MissingApiKey { .. }
            | LlmError::Stream { .. }
            | LlmError::InputTooLarge { .. } => false,
        }
    }
}

/// A provider-agnostic classification of an API error's HTTP status, per
/// `shared/error-codes.md`'s table (400/401/403/404/413/429/500/529).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiErrorKind {
    InvalidRequest,
    Authentication,
    Permission,
    NotFound,
    RequestTooLarge,
    RateLimit,
    /// 5xx other than the Anthropic-specific 529.
    Server,
    /// Anthropic's `529 overloaded_error`. Not a registered HTTP status, so
    /// it needs its own arm rather than folding into `Server`.
    Overloaded,
    Other,
}

impl ApiErrorKind {
    pub fn from_status(status: u16) -> Self {
        match status {
            400 => Self::InvalidRequest,
            401 => Self::Authentication,
            403 => Self::Permission,
            404 => Self::NotFound,
            413 => Self::RequestTooLarge,
            429 => Self::RateLimit,
            529 => Self::Overloaded,
            500..=599 => Self::Server,
            _ => Self::Other,
        }
    }

    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::RateLimit | Self::Server | Self::Overloaded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_known_statuses() {
        assert_eq!(ApiErrorKind::from_status(400), ApiErrorKind::InvalidRequest);
        assert_eq!(ApiErrorKind::from_status(401), ApiErrorKind::Authentication);
        assert_eq!(ApiErrorKind::from_status(403), ApiErrorKind::Permission);
        assert_eq!(ApiErrorKind::from_status(404), ApiErrorKind::NotFound);
        assert_eq!(ApiErrorKind::from_status(413), ApiErrorKind::RequestTooLarge);
        assert_eq!(ApiErrorKind::from_status(429), ApiErrorKind::RateLimit);
        assert_eq!(ApiErrorKind::from_status(500), ApiErrorKind::Server);
        assert_eq!(ApiErrorKind::from_status(503), ApiErrorKind::Server);
        assert_eq!(ApiErrorKind::from_status(529), ApiErrorKind::Overloaded);
        assert_eq!(ApiErrorKind::from_status(418), ApiErrorKind::Other);
    }

    #[test]
    fn classifies_429_and_529_as_retryable() {
        assert!(ApiErrorKind::RateLimit.is_retryable());
        assert!(ApiErrorKind::Overloaded.is_retryable());
        assert!(ApiErrorKind::Server.is_retryable());
        assert!(!ApiErrorKind::InvalidRequest.is_retryable());
        assert!(!ApiErrorKind::Authentication.is_retryable());
        assert!(!ApiErrorKind::NotFound.is_retryable());
    }

    #[test]
    fn does_not_retry_a_400() {
        let err = LlmError::Status {
            provider: "test",
            status: 400,
            kind: ApiErrorKind::from_status(400),
            message: "bad request".to_string(),
            request_id: None,
        };
        assert!(!err.is_retryable());
    }

    #[test]
    fn a_parse_error_is_never_retryable() {
        let err = LlmError::Parse {
            provider: "ollama",
            context: "library index".to_string(),
            message: "no model cards found".to_string(),
        };
        assert!(!err.is_retryable());
        assert!(err.to_string().contains("no model cards found"));
    }

    #[test]
    fn missing_api_key_names_the_variable_in_its_message() {
        let err = LlmError::MissingApiKey {
            var: "ANTHROPIC_API_KEY".to_string(),
        };
        assert!(err.to_string().contains("ANTHROPIC_API_KEY"));
        assert!(!err.is_retryable());
    }

    #[test]
    fn input_too_large_is_never_retryable() {
        let err = LlmError::InputTooLarge {
            provider: "openai",
            model: "text-embedding-3-small".to_string(),
            max_input_tokens: Some(8192),
            message: "input exceeds the maximum context length".to_string(),
        };
        assert!(!err.is_retryable());
        assert!(err.to_string().contains("text-embedding-3-small"));
        assert!(err.to_string().contains("exceeds the maximum context length"));
    }
}
