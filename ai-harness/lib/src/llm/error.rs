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
        /// The provider's own error identifier, when its envelope carries
        /// one — Anthropic's `error.type`, OpenAI's `error.code` (falling
        /// back to its `error.type`). `None` for Ollama's bare
        /// `{"error": "..."}` body, or when a body didn't parse as the
        /// expected envelope at all.
        code: Option<String>,
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
        /// The same classification an HTTP failure would carry, derived
        /// from the provider's own error type on the stream frame (there is
        /// no HTTP status to fall back to mid-stream — the connection
        /// already succeeded). `ApiErrorKind::Other` when the frame names no
        /// code this crate recognizes.
        kind: ApiErrorKind,
        message: String,
    },

    /// An embeddings or chat request's input was too large for the model —
    /// a provider's own rejection, reclassified from a plain
    /// [`LlmError::Status`] by [`ApiError::classify`]. Never raised
    /// speculatively: this crate never estimates a request's size and
    /// refuses it locally, only reclassifies what the API already rejected.
    /// See `shared::llm::ModelDetails::probably_fits` for an opt-in, purely
    /// advisory pre-flight estimate a caller may use instead.
    #[error("{provider}: input too large for {model}: {message}")]
    InputTooLarge {
        provider: &'static str,
        model: String,
        /// The model's limit in tokens. Currently always `None`: neither
        /// provider's rejection message states the limit in a form worth
        /// parsing back out (see each `classify` call site), and this crate
        /// never looks it up from the model listing to fill this in. A
        /// caller wanting the number should read it off
        /// `ModelDetails::max_input_tokens()` instead, which does report one
        /// for `ModelDetails::OllamaLocalEmbedding` and
        /// `ModelDetails::Anthropic`.
        max_input_tokens: Option<u64>,
        message: String,
    },

    /// The provider accepted the request's shape but refuses to bill it:
    /// credits exhausted, a hard spend limit reached, or a suspended
    /// account — reclassified from the 400 (Anthropic) or 429 (OpenAI) the
    /// provider actually sent by [`ApiError::classify`] /
    /// [`classify_stream_error`]. Notably *not* retryable despite OpenAI's
    /// 429 status: the fix is out-of-band (top up the account), and
    /// retrying burns attempts on a failure that cannot succeed.
    #[error("{provider}: insufficient credit: {message}")]
    InsufficientCredit {
        provider: &'static str,
        message: String,
    },

    /// `Router::chat`/`embeddings` was asked for a provider name nothing was
    /// ever registered under.
    #[error("unknown provider {name:?}; registered: {}", available.join(", "))]
    UnknownProvider { name: String, available: Vec<String> },

    /// The named provider is registered, but not with the capability being
    /// asked for (e.g. `router.embeddings(...)` on a provider registered
    /// chat-only).
    #[error("provider {provider:?} does not support {capability}")]
    Unsupported {
        provider: String,
        capability: &'static str,
    },
}

impl LlmError {
    /// Whether retrying the same request might succeed. Config errors,
    /// decode errors, and 4xx statuses (other than 429) are not — retrying
    /// them would just fail the same way again. Neither is
    /// [`LlmError::InsufficientCredit`], regardless of the HTTP status it
    /// was reclassified from: no amount of retrying tops up an account.
    pub fn is_retryable(&self) -> bool {
        match self {
            LlmError::Status { kind, .. } => kind.is_retryable(),
            LlmError::Stream { kind, .. } => kind.is_retryable(),
            // A connection-level failure (DNS, TCP reset, TLS handshake) is
            // usually transient and worth one more attempt.
            LlmError::Http { .. } => true,
            LlmError::Decode { .. }
            | LlmError::Parse { .. }
            | LlmError::MissingApiKey { .. }
            | LlmError::InputTooLarge { .. }
            | LlmError::InsufficientCredit { .. }
            | LlmError::UnknownProvider { .. }
            | LlmError::Unsupported { .. } => false,
        }
    }

    /// The provider this error came from, when there is one — absent for
    /// the two variants that never reached a provider (`Router` couldn't
    /// even resolve one to try).
    fn provider(&self) -> Option<&'static str> {
        match self {
            LlmError::Http { provider, .. }
            | LlmError::Status { provider, .. }
            | LlmError::Decode { provider, .. }
            | LlmError::Parse { provider, .. }
            | LlmError::Stream { provider, .. }
            | LlmError::InputTooLarge { provider, .. }
            | LlmError::InsufficientCredit { provider, .. } => Some(provider),
            LlmError::MissingApiKey { .. }
            | LlmError::UnknownProvider { .. }
            | LlmError::Unsupported { .. } => None,
        }
    }
}

fn report_kind_for_api_kind(kind: ApiErrorKind) -> shared::error::ErrorKind {
    use shared::error::ErrorKind;
    match kind {
        ApiErrorKind::RateLimit => ErrorKind::RateLimit,
        ApiErrorKind::Authentication | ApiErrorKind::Permission => ErrorKind::Auth,
        ApiErrorKind::InvalidRequest | ApiErrorKind::RequestTooLarge => ErrorKind::InvalidRequest,
        ApiErrorKind::NotFound => ErrorKind::NotFound,
        ApiErrorKind::Server | ApiErrorKind::Overloaded => ErrorKind::Server,
        ApiErrorKind::Other => ErrorKind::Other,
    }
}

/// Flatten an `LlmError` into the DTO that can cross the Tauri IPC boundary
/// — see `shared::error`'s module doc for why the native type itself can't.
impl From<&LlmError> for shared::error::ErrorReport {
    fn from(err: &LlmError) -> Self {
        use shared::error::ErrorKind;
        let kind = match err {
            LlmError::InsufficientCredit { .. } => ErrorKind::InsufficientCredit,
            LlmError::InputTooLarge { .. } => ErrorKind::InputTooLarge,
            LlmError::Status { kind, .. } | LlmError::Stream { kind, .. } => {
                report_kind_for_api_kind(*kind)
            }
            LlmError::Http { .. } => ErrorKind::Network,
            LlmError::MissingApiKey { .. }
            | LlmError::UnknownProvider { .. }
            | LlmError::Unsupported { .. } => ErrorKind::Config,
            LlmError::Decode { .. } | LlmError::Parse { .. } => ErrorKind::Other,
        };
        Self {
            kind,
            provider: err.provider().map(str::to_string),
            message: err.to_string(),
            retryable: err.is_retryable(),
        }
    }
}

/// A provider-agnostic classification of an API error, derived from either
/// its HTTP status ([`ApiErrorKind::from_status`]) or a mid-stream frame's
/// own error type, when the provider names one this crate recognizes
/// ([`ApiErrorKind::from_provider_code`]).
///
/// Status mapping: 400 → InvalidRequest, 401 → Authentication,
/// 403 → Permission, 404 → NotFound, 413 → RequestTooLarge, 429 → RateLimit,
/// 500-599 → Server (529 is the Anthropic-specific `Overloaded` exception),
/// everything else → Other.
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

    /// Map a provider's own error-type identifier to a classification —
    /// the only classification available for a mid-stream failure, which
    /// carries no HTTP status. Covers the vocabulary Anthropic and OpenAI
    /// actually use on the wire; an identifier this crate doesn't recognize
    /// yields `None` rather than a guess.
    pub fn from_provider_code(code: &str) -> Option<Self> {
        match code {
            "overloaded_error" => Some(Self::Overloaded),
            "rate_limit_error" | "rate_limit_exceeded" => Some(Self::RateLimit),
            "invalid_request_error" => Some(Self::InvalidRequest),
            "authentication_error" | "invalid_api_key" => Some(Self::Authentication),
            "permission_error" => Some(Self::Permission),
            "not_found_error" => Some(Self::NotFound),
            "request_too_large" => Some(Self::RequestTooLarge),
            "api_error" | "server_error" => Some(Self::Server),
            _ => None,
        }
    }

    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::RateLimit | Self::Server | Self::Overloaded)
    }
}

/// Message substrings that identify an "input too large" rejection,
/// regardless of provider. Case-insensitive, and kept as one list rather
/// than one per provider — the wording is disjoint across providers, so a
/// phrase that starts appearing on one provider's error protects the
/// others too.
///
/// - OpenAI (older wording): "This model's maximum context length is 8192
///   tokens, however you requested ... tokens"
/// - OpenAI (embeddings wording): "Invalid 'input[0]': maximum input length
///   is 8192 tokens."
/// - Ollama (`truncate: false`): "the input length exceeds the context
///   length"
const INPUT_TOO_LARGE_SIGNATURES: &[&str] = &[
    "maximum context length",
    "maximum input length",
    "exceeds the context length",
];

/// `error.code`/`error.type` values that identify an "input too large"
/// rejection.
const INPUT_TOO_LARGE_CODES: &[&str] = &["context_length_exceeded"];

/// `error.code`/`error.type` values that identify a billing/quota
/// rejection — OpenAI: `insufficient_quota`, `billing_hard_limit_reached`.
/// Anthropic sends no distinguishing code for this case (its `error.type`
/// stays `invalid_request_error`); the message signatures below cover it
/// instead.
const INSUFFICIENT_CREDIT_CODES: &[&str] =
    &["insufficient_quota", "billing_hard_limit_reached", "billing_error"];

/// Message substrings that identify a billing/quota rejection when no code
/// names it.
///
/// - Anthropic: "Your credit balance is too low to access the Anthropic
///   API..."
/// - OpenAI: "You exceeded your current quota..."
const INSUFFICIENT_CREDIT_SIGNATURES: &[&str] = &[
    "credit balance is too low",
    "exceeded your current quota",
    "billing hard limit",
];

fn message_matches(message: &str, signatures: &[&str]) -> bool {
    let lower = message.to_lowercase();
    signatures.iter().any(|sig| lower.contains(sig))
}

fn code_matches(code: Option<&str>, codes: &[&str]) -> bool {
    code.is_some_and(|c| codes.contains(&c))
}

/// One provider's HTTP error response, parsed but not yet classified into a
/// specific [`LlmError`] variant. Every provider's `wire::parse_error`
/// builds one of these and calls [`ApiError::classify`] — the one place
/// billing/input-size reclassification happens, so a provider's wire module
/// only needs to know how to parse its own envelope shape. See
/// [`classify_stream_error`] for the mid-stream counterpart (no HTTP status
/// applies there).
pub struct ApiError {
    pub provider: &'static str,
    pub status: u16,
    pub code: Option<String>,
    pub message: String,
    pub request_id: Option<String>,
    /// The model the request named, when the caller knows it. Required to
    /// raise [`LlmError::InputTooLarge`], which reports it; `None` on paths
    /// that don't name one (model listings, image generation, the Ollama
    /// library scrape).
    pub model: Option<String>,
}

impl ApiError {
    /// Promote the errors that deserve a dedicated variant — billing first
    /// (an out-of-credit rejection is also, technically, a plain
    /// `invalid_request_error`/400, so it must be checked before input-size
    /// or it would never be reached), then input-size, then fall back to a
    /// plain [`LlmError::Status`].
    pub fn classify(self) -> LlmError {
        if code_matches(self.code.as_deref(), INSUFFICIENT_CREDIT_CODES)
            || message_matches(&self.message, INSUFFICIENT_CREDIT_SIGNATURES)
        {
            return LlmError::InsufficientCredit {
                provider: self.provider,
                message: self.message,
            };
        }

        if let Some(model) = self.model {
            if code_matches(self.code.as_deref(), INPUT_TOO_LARGE_CODES)
                || message_matches(&self.message, INPUT_TOO_LARGE_SIGNATURES)
            {
                return LlmError::InputTooLarge {
                    provider: self.provider,
                    model,
                    max_input_tokens: None,
                    message: self.message,
                };
            }
        }

        let kind = self
            .code
            .as_deref()
            .and_then(ApiErrorKind::from_provider_code)
            .unwrap_or_else(|| ApiErrorKind::from_status(self.status));

        LlmError::Status {
            provider: self.provider,
            status: self.status,
            kind,
            code: self.code,
            message: self.message,
            request_id: self.request_id,
        }
    }
}

/// The mid-stream counterpart of [`ApiError::classify`]: a stream frame
/// carries no HTTP status (the connection already succeeded) and, in this
/// crate's stream-translation functions, no model — in practice a
/// context-length rejection always arrives before generation starts, never
/// mid-stream, so there is no input-size reclassification to do here, only
/// billing.
pub fn classify_stream_error(provider: &'static str, code: Option<String>, message: String) -> LlmError {
    if code_matches(code.as_deref(), INSUFFICIENT_CREDIT_CODES)
        || message_matches(&message, INSUFFICIENT_CREDIT_SIGNATURES)
    {
        return LlmError::InsufficientCredit { provider, message };
    }

    let kind = code
        .as_deref()
        .and_then(ApiErrorKind::from_provider_code)
        .unwrap_or(ApiErrorKind::Other);

    LlmError::Stream {
        provider,
        kind,
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ERROR_CONTEXT_LENGTH: &str = include_str!("openai/fixtures/error_context_length.json");
    const ERROR_INPUT_TOO_LONG: &str = include_str!("openai/fixtures/error_input_too_long.json");
    const ERROR_INSUFFICIENT_QUOTA: &str =
        include_str!("openai/fixtures/error_insufficient_quota.json");
    const OPENAI_ERROR: &str = include_str!("openai/fixtures/error.json");

    const ERROR_BILLING: &str = include_str!("anthropic/fixtures/error_billing.json");

    const OLLAMA_ERROR_INPUT_TOO_LONG: &str =
        include_str!("ollama/fixtures/error_input_too_long.json");
    const OLLAMA_ERROR: &str = include_str!("ollama/fixtures/error.json");

    fn openai_error(status: u16, body: &str, model: &str) -> LlmError {
        crate::llm::openai::wire::parse_error_for_model(status, body, model)
    }

    fn openai_error_no_model(status: u16, body: &str) -> LlmError {
        crate::llm::openai::wire::parse_error(status, body)
    }

    fn anthropic_error(status: u16, body: &str) -> LlmError {
        crate::llm::anthropic::wire::parse_error(status, body)
    }

    fn ollama_error(status: u16, body: &str, model: &str) -> LlmError {
        crate::llm::ollama::wire::parse_error_for_model(status, body, model)
    }

    fn ollama_error_no_model(status: u16, body: &str) -> LlmError {
        crate::llm::ollama::wire::parse_error(status, body)
    }

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
    fn from_provider_code_maps_the_known_vocabulary() {
        assert_eq!(
            ApiErrorKind::from_provider_code("overloaded_error"),
            Some(ApiErrorKind::Overloaded)
        );
        assert_eq!(
            ApiErrorKind::from_provider_code("rate_limit_error"),
            Some(ApiErrorKind::RateLimit)
        );
        assert_eq!(
            ApiErrorKind::from_provider_code("invalid_request_error"),
            Some(ApiErrorKind::InvalidRequest)
        );
        assert_eq!(ApiErrorKind::from_provider_code("something_new"), None);
    }

    #[test]
    fn does_not_retry_a_400() {
        let err = LlmError::Status {
            provider: "test",
            status: 400,
            kind: ApiErrorKind::from_status(400),
            code: None,
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

    // -- ErrorReport: the IPC-safe flattening -------------------------------

    #[test]
    fn insufficient_credit_reports_as_insufficient_credit_and_not_retryable() {
        let err = LlmError::InsufficientCredit {
            provider: "openai",
            message: "you exceeded your current quota".to_string(),
        };
        let report: shared::error::ErrorReport = (&err).into();
        assert_eq!(report.kind, shared::error::ErrorKind::InsufficientCredit);
        assert_eq!(report.provider.as_deref(), Some("openai"));
        assert!(!report.retryable);
    }

    #[test]
    fn a_retryable_status_reports_as_retryable() {
        let err = LlmError::Status {
            provider: "openai",
            status: 500,
            kind: ApiErrorKind::Server,
            code: None,
            message: "internal error".to_string(),
            request_id: None,
        };
        let report: shared::error::ErrorReport = (&err).into();
        assert_eq!(report.kind, shared::error::ErrorKind::Server);
        assert!(report.retryable);
    }

    #[test]
    fn a_config_error_reports_with_no_provider() {
        let err = LlmError::MissingApiKey {
            var: "ANTHROPIC_API_KEY".to_string(),
        };
        let report: shared::error::ErrorReport = (&err).into();
        assert_eq!(report.kind, shared::error::ErrorKind::Config);
        assert_eq!(report.provider, None);
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
    fn unknown_provider_names_what_is_registered() {
        let err = LlmError::UnknownProvider {
            name: "azure".to_string(),
            available: vec!["anthropic".to_string(), "ollama".to_string()],
        };
        assert!(!err.is_retryable());
        let message = err.to_string();
        assert!(message.contains("azure"));
        assert!(message.contains("anthropic"));
        assert!(message.contains("ollama"));
    }

    #[test]
    fn unsupported_names_the_provider_and_capability() {
        let err = LlmError::Unsupported {
            provider: "anthropic".to_string(),
            capability: "embeddings",
        };
        assert!(!err.is_retryable());
        assert!(err.to_string().contains("anthropic"));
        assert!(err.to_string().contains("embeddings"));
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

    // -- InsufficientCredit: the regression coverage for the actual bug --
    // a 429 that must NOT be retried. --------------------------------------

    #[test]
    fn a_429_insufficient_quota_becomes_insufficient_credit() {
        let err = openai_error_no_model(429, ERROR_INSUFFICIENT_QUOTA);
        match err {
            LlmError::InsufficientCredit { provider, .. } => assert_eq!(provider, "openai"),
            other => panic!("expected InsufficientCredit, got {other:?}"),
        }
        assert!(!err.is_retryable(), "a billing failure must never be retried");
    }

    #[test]
    fn an_anthropic_credit_balance_400_becomes_insufficient_credit() {
        let err = anthropic_error(400, ERROR_BILLING);
        match &err {
            LlmError::InsufficientCredit { provider, message } => {
                assert_eq!(*provider, "anthropic");
                assert!(message.contains("credit balance is too low"));
            }
            other => panic!("expected InsufficientCredit, got {other:?}"),
        }
        assert!(!err.is_retryable());
    }

    /// A 429 with no provider code naming a billing reason (so nothing for
    /// `from_provider_code` to override the status with) must still be an
    /// ordinary, retryable rate limit — the billing check must not be so
    /// broad that every 429 becomes `InsufficientCredit`.
    #[test]
    fn an_ordinary_429_is_still_a_retryable_rate_limit() {
        let err = ApiError {
            provider: "openai",
            status: 429,
            code: None,
            message: "Rate limit reached for requests".to_string(),
            request_id: None,
            model: None,
        }
        .classify();
        match err {
            LlmError::Status { kind, .. } => assert_eq!(kind, ApiErrorKind::RateLimit),
            other => panic!("expected Status, got {other:?}"),
        }
        assert!(err.is_retryable());
    }

    // -- InputTooLarge: reclassification now applies on the chat path too,
    // not only embeddings. ---------------------------------------------

    #[test]
    fn a_context_length_error_becomes_input_too_large_on_the_chat_path() {
        let err = openai_error(400, ERROR_CONTEXT_LENGTH, "gpt-5.6");
        match err {
            LlmError::InputTooLarge { provider, model, .. } => {
                assert_eq!(provider, "openai");
                assert_eq!(model, "gpt-5.6");
            }
            other => panic!("expected InputTooLarge, got {other:?}"),
        }
    }

    #[test]
    fn an_input_too_long_error_becomes_input_too_large() {
        let err = openai_error(400, ERROR_INPUT_TOO_LONG, "text-embedding-3-small");
        match err {
            LlmError::InputTooLarge { provider, model, .. } => {
                assert_eq!(provider, "openai");
                assert_eq!(model, "text-embedding-3-small");
            }
            other => panic!("expected InputTooLarge, got {other:?}"),
        }
    }

    #[test]
    fn an_unrelated_openai_error_stays_a_plain_status() {
        let err = openai_error(400, OPENAI_ERROR, "text-embedding-3-small");
        match err {
            LlmError::Status { status, .. } => assert_eq!(status, 400),
            other => panic!("expected Status, got {other:?}"),
        }
    }

    #[test]
    fn without_a_model_a_too_long_error_stays_a_plain_status() {
        // Model listings and image generation never name a model in the
        // sense `InputTooLarge` reports one — reclassification must not
        // fire when there's nothing to report.
        let err = openai_error_no_model(400, ERROR_CONTEXT_LENGTH);
        match err {
            LlmError::Status { status, .. } => assert_eq!(status, 400),
            other => panic!("expected Status, got {other:?}"),
        }
    }

    #[test]
    fn ollama_context_length_error_becomes_input_too_large() {
        let err = ollama_error(400, OLLAMA_ERROR_INPUT_TOO_LONG, "nomic-embed-text");
        match err {
            LlmError::InputTooLarge { provider, model, .. } => {
                assert_eq!(provider, "ollama");
                assert_eq!(model, "nomic-embed-text");
            }
            other => panic!("expected InputTooLarge, got {other:?}"),
        }
    }

    #[test]
    fn ollama_model_not_found_stays_a_plain_status() {
        let err = ollama_error(404, OLLAMA_ERROR, "nonexistent:latest");
        match err {
            LlmError::Status { status, .. } => assert_eq!(status, 404),
            other => panic!("expected Status, got {other:?}"),
        }
        assert!(matches!(
            ollama_error_no_model(404, OLLAMA_ERROR),
            LlmError::Status { .. }
        ));
    }
}
