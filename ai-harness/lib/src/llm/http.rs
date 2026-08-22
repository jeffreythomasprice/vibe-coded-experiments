//! Shared HTTP plumbing: client construction and the retry loop.
//!
//! Per-provider request building and response parsing lives in each
//! provider's `wire` module (pure functions over `&str`/`&[u8]`, unit-tested
//! on fixtures); this module is the one place that actually issues requests,
//! and it stays as thin as possible so there's little here that *can't* be
//! unit-tested — see `retry_delay` and `parse_retry_after` below.

use std::time::Duration;

use reqwest::Client;

use crate::llm::error::{ApiErrorKind, LlmError};

/// Build a client with the given per-request timeout. One client is shared
/// across every request a provider makes, so connections can be reused.
pub fn build_client(timeout: Duration) -> Result<Client, LlmError> {
    Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|source| LlmError::Http {
            provider: "http",
            source,
        })
}

/// Send `request`, retrying a retryable failure up to `max_retries` times.
///
/// On a non-retryable failure, or once retries are exhausted, `parse_error`
/// turns the status and response body into a provider-shaped `LlmError` —
/// each provider's `wire::parse_error` knows how to pull a message and
/// request ID out of its own error envelope; this function only needs the
/// HTTP status to decide whether to retry at all.
pub async fn send_with_retry(
    provider: &'static str,
    request: reqwest::RequestBuilder,
    max_retries: u32,
    parse_error: impl Fn(u16, &str) -> LlmError,
) -> Result<reqwest::Response, LlmError> {
    let mut attempt = 0;
    loop {
        let attempt_request = request.try_clone().expect(
            "request body must be clonable to support retries (no streaming request bodies)",
        );
        let response = attempt_request
            .send()
            .await
            .map_err(|source| LlmError::Http { provider, source })?;

        if response.status().is_success() {
            return Ok(response);
        }

        let status = response.status().as_u16();
        let kind = ApiErrorKind::from_status(status);
        let retry_after = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(parse_retry_after);

        match retry_delay(&kind, retry_after, attempt, max_retries) {
            Some(delay) => {
                tracing::warn!(
                    provider,
                    status,
                    attempt,
                    delay_ms = delay.as_millis() as u64,
                    "retrying after a retryable provider error"
                );
                tokio::time::sleep(delay).await;
                attempt += 1;
            }
            None => {
                let body = response.text().await.unwrap_or_default();
                return Err(parse_error(status, &body));
            }
        }
    }
}

/// How long to wait before the next attempt, or `None` to give up.
///
/// Pure — no clock, no network — so it's directly unit-testable, the same
/// trick `RotatingWriter::write_at` uses with an injected `now`.
pub fn retry_delay(
    kind: &ApiErrorKind,
    retry_after: Option<Duration>,
    attempt: u32,
    max_retries: u32,
) -> Option<Duration> {
    if !kind.is_retryable() || attempt >= max_retries {
        return None;
    }
    if let Some(delay) = retry_after {
        return Some(delay);
    }
    // Exponential backoff with no server-provided hint: 500ms, 1s, 2s, ...
    Some(Duration::from_millis(500 * 2u64.saturating_pow(attempt)))
}

/// Parse a `retry-after` header value. Only the integer-seconds form (RFC
/// 9110 §10.2.3) is implemented; the HTTP-date alternative isn't sent by any
/// provider this crate talks to.
pub fn parse_retry_after(value: &str) -> Option<Duration> {
    value.trim().parse::<u64>().ok().map(Duration::from_secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn does_not_retry_a_400() {
        let kind = ApiErrorKind::InvalidRequest;
        assert_eq!(retry_delay(&kind, None, 0, 3), None);
    }

    #[test]
    fn honours_retry_after_over_computed_backoff() {
        let kind = ApiErrorKind::RateLimit;
        let delay = retry_delay(&kind, Some(Duration::from_secs(7)), 0, 3);
        assert_eq!(delay, Some(Duration::from_secs(7)));
    }

    #[test]
    fn backs_off_exponentially_without_a_retry_after_hint() {
        let kind = ApiErrorKind::Server;
        assert_eq!(retry_delay(&kind, None, 0, 5), Some(Duration::from_millis(500)));
        assert_eq!(retry_delay(&kind, None, 1, 5), Some(Duration::from_millis(1000)));
        assert_eq!(retry_delay(&kind, None, 2, 5), Some(Duration::from_millis(2000)));
    }

    #[test]
    fn gives_up_after_the_configured_attempt_count() {
        let kind = ApiErrorKind::Overloaded;
        assert_eq!(retry_delay(&kind, None, 3, 3), None);
        assert!(retry_delay(&kind, None, 2, 3).is_some());
    }

    #[test]
    fn zero_max_retries_disables_retrying_entirely() {
        let kind = ApiErrorKind::RateLimit;
        assert_eq!(retry_delay(&kind, None, 0, 0), None);
    }

    #[test]
    fn parses_integer_retry_after_seconds() {
        assert_eq!(parse_retry_after("30"), Some(Duration::from_secs(30)));
        assert_eq!(parse_retry_after(" 5 "), Some(Duration::from_secs(5)));
    }

    #[test]
    fn ignores_an_http_date_retry_after() {
        // Not implemented — no provider here sends it — but it must not panic.
        assert_eq!(parse_retry_after("Tue, 29 Oct 2026 16:00:00 GMT"), None);
    }
}
