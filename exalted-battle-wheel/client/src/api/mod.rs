//! The server's access-code HTTP API (`server/src/routes.rs`), from the client side. Every
//! function takes the bearer token explicitly -- this module holds no state of its own; see
//! `crate::access` for the reactive facade built on top of it.

mod error;

pub use error::ApiError;

use crate::config::API_BASE_URL;
use gloo_net::http::{Request, RequestBuilder, Response};
use shared::access::{AccessCode, AccessCodeList, CreateAccessCode, UpdateAccessCode};

fn endpoint(path: &str) -> String {
    format!("{}{path}", API_BASE_URL.trim_end_matches('/'))
}

/// Percent-encodes a single path segment (RFC 3986's unreserved set survives unescaped) so an
/// access code containing `/`, `#`, `?`, or whitespace still round-trips through
/// `/access-codes/{access_key}`. Hand-rolled rather than `js_sys::encode_uri_component` so it's
/// plain, host-testable logic -- this crate has no wasm-bindgen-test setup.
fn percent_encode_segment(segment: &str) -> String {
    let mut encoded = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => encoded.push(byte as char),
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn bearer(builder: RequestBuilder, token: &str) -> RequestBuilder {
    builder.header("authorization", &format!("Bearer {token}"))
}

async fn body_of(response: &Response) -> String {
    response.text().await.unwrap_or_default()
}

async fn parse_json<T: serde::de::DeserializeOwned>(response: Response) -> Result<T, ApiError> {
    if !response.ok() {
        return Err(error::error_for(response.status(), &body_of(&response).await));
    }
    response.json().await.map_err(|error| ApiError::Malformed(error.to_string()))
}

async fn expect_no_content(response: Response) -> Result<(), ApiError> {
    if response.ok() {
        return Ok(());
    }
    Err(error::error_for(response.status(), &body_of(&response).await))
}

pub async fn me(token: &str) -> Result<AccessCode, ApiError> {
    let request = bearer(Request::get(&endpoint("/auth/me")), token);
    let response = request.send().await.map_err(|error| ApiError::Transport(error.to_string()))?;
    parse_json(response).await
}

pub async fn list(token: &str) -> Result<Vec<AccessCode>, ApiError> {
    let request = bearer(Request::get(&endpoint("/access-codes")), token);
    let response = request.send().await.map_err(|error| ApiError::Transport(error.to_string()))?;
    let list: AccessCodeList = parse_json(response).await?;
    Ok(list.codes)
}

pub async fn create(token: &str, body: &CreateAccessCode) -> Result<AccessCode, ApiError> {
    let request = bearer(Request::post(&endpoint("/access-codes")), token)
        .json(body)
        .map_err(|error| ApiError::Malformed(error.to_string()))?;
    let response = request.send().await.map_err(|error| ApiError::Transport(error.to_string()))?;
    parse_json(response).await
}

pub async fn update(token: &str, access_key: &str, is_admin: bool) -> Result<AccessCode, ApiError> {
    let path = format!("/access-codes/{}", percent_encode_segment(access_key));
    let request = bearer(Request::put(&endpoint(&path)), token)
        .json(&UpdateAccessCode { is_admin })
        .map_err(|error| ApiError::Malformed(error.to_string()))?;
    let response = request.send().await.map_err(|error| ApiError::Transport(error.to_string()))?;
    parse_json(response).await
}

pub async fn delete(token: &str, access_key: &str) -> Result<(), ApiError> {
    let path = format!("/access-codes/{}", percent_encode_segment(access_key));
    let request = bearer(Request::delete(&endpoint(&path)), token);
    let response = request.send().await.map_err(|error| ApiError::Transport(error.to_string()))?;
    expect_no_content(response).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_joins_the_base_and_path() {
        assert_eq!(endpoint("/auth/me"), format!("{API_BASE_URL}/auth/me"));
    }

    #[test]
    fn percent_encoding_leaves_unreserved_characters_alone() {
        assert_eq!(percent_encode_segment("player-one.2_3~"), "player-one.2_3~");
    }

    #[test]
    fn percent_encoding_escapes_everything_else() {
        assert_eq!(percent_encode_segment("a/b c#d"), "a%2Fb%20c%23d");
    }
}
