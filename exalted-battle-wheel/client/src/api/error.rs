use shared::access::ApiErrorBody;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ApiError {
    #[error("could not reach the server: {0}")]
    Transport(String),
    #[error("that access code was not recognized")]
    Unauthorized,
    // Carries the server's own message so "cannot modify or delete your own access code" (the
    // self-modification case) reads correctly with no server-side error code needed.
    #[error("{0}")]
    Forbidden(String),
    #[error("that access code no longer exists")]
    NotFound,
    #[error("that access code already exists")]
    Conflict,
    #[error("the server had a problem (status {status})")]
    Server { status: u16 },
    #[error("unexpected response status {status}")]
    Unexpected { status: u16 },
    #[error("could not read the server's response: {0}")]
    Malformed(String),
}

/// The server's own message, if the body parses as `{"error": "..."}`; otherwise the raw body,
/// trimmed, so a proxy's plain-text error page is still readable rather than silently dropped.
fn message(body: &str) -> String {
    serde_json::from_str::<ApiErrorBody>(body).map(|error| error.error).unwrap_or_else(|_| body.trim().to_string())
}

pub fn error_for(status: u16, body: &str) -> ApiError {
    match status {
        401 => ApiError::Unauthorized,
        403 => ApiError::Forbidden(message(body)),
        404 => ApiError::NotFound,
        409 => ApiError::Conflict,
        500..=599 => ApiError::Server { status },
        _ => ApiError::Unexpected { status },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_known_statuses() {
        assert_eq!(error_for(401, ""), ApiError::Unauthorized);
        assert_eq!(error_for(404, ""), ApiError::NotFound);
        assert_eq!(error_for(409, ""), ApiError::Conflict);
        assert_eq!(error_for(500, ""), ApiError::Server { status: 500 });
        assert_eq!(error_for(418, ""), ApiError::Unexpected { status: 418 });
    }

    #[test]
    fn forbidden_carries_the_servers_message() {
        let error = error_for(403, r#"{"error":"cannot modify or delete your own access code"}"#);
        assert_eq!(error, ApiError::Forbidden("cannot modify or delete your own access code".to_string()));
    }

    #[test]
    fn forbidden_falls_back_to_the_raw_body_when_not_json() {
        let error = error_for(403, "  access denied  \n");
        assert_eq!(error, ApiError::Forbidden("access denied".to_string()));
    }
}
