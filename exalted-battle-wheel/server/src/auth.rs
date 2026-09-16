//! Bearer-token authentication and admin authorization, as axum middleware. `require_access_code`
//! looks up the token and stashes the resulting `AccessCode` in the request's extensions;
//! `require_admin` reads it back and checks `is_admin`. The `FromRequestParts` impl lets a handler
//! just take an `AccessCode` argument instead of repeating either check.

use crate::access_codes::{AccessCode, AccessCodeStore};
use crate::error::ApiError;
use crate::routes::AppState;
use axum::extract::{FromRequestParts, Request, State};
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use axum::http::HeaderMap;
use axum::middleware::Next;
use axum::response::Response;

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let (scheme, token) = headers.get(AUTHORIZATION)?.to_str().ok()?.split_once(' ')?;
    scheme.eq_ignore_ascii_case("bearer").then(|| token.trim())
}

pub async fn require_access_code<S: AccessCodeStore>(
    State(state): State<AppState<S>>,
    mut request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let Some(token) = bearer_token(request.headers()) else {
        return Err(ApiError::Unauthorized);
    };

    let Some(code) = state.access_codes.get(token).await? else {
        tracing::debug!("rejected an unknown access code");
        return Err(ApiError::Unauthorized);
    };

    request.extensions_mut().insert(code);
    Ok(next.run(request).await)
}

pub async fn require_admin(request: Request, next: Next) -> Result<Response, ApiError> {
    let code = request.extensions().get::<AccessCode>().cloned().ok_or(ApiError::Unauthorized)?;
    if !code.is_admin {
        return Err(ApiError::Forbidden);
    }
    Ok(next.run(request).await)
}

impl<S> FromRequestParts<S> for AccessCode
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts.extensions.get::<AccessCode>().cloned().ok_or(ApiError::Unauthorized)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, HeaderValue::from_str(value).unwrap());
        headers
    }

    #[test]
    fn accepts_bearer_case_insensitively() {
        assert_eq!(bearer_token(&headers("bearer abc123")), Some("abc123"));
        assert_eq!(bearer_token(&headers("Bearer abc123")), Some("abc123"));
        assert_eq!(bearer_token(&headers("BEARER abc123")), Some("abc123"));
    }

    #[test]
    fn rejects_other_schemes() {
        assert_eq!(bearer_token(&headers("Basic abc123")), None);
    }

    #[test]
    fn rejects_missing_header() {
        assert_eq!(bearer_token(&HeaderMap::new()), None);
    }

    #[test]
    fn rejects_malformed_header() {
        assert_eq!(bearer_token(&headers("abc123")), None);
    }
}
