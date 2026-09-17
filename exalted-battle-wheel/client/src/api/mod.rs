//! The server's access-code HTTP API (`server/src/routes.rs`), from the client side. Every
//! function takes the bearer token explicitly -- this module holds no state of its own; see
//! `crate::access` for the reactive facade built on top of it.

mod error;

pub use error::ApiError;

use crate::config::API_BASE_URL;
use gloo_net::http::{Request, RequestBuilder, Response};
use shared::access::{AccessCode, AccessCodeList, CreateAccessCode, UpdateAccessCode};
use shared::rooms::RoomList;

fn endpoint(path: &str) -> String {
    format!("{}{path}", API_BASE_URL.trim_end_matches('/'))
}

/// Percent-encodes a single path segment (RFC 3986's unreserved set survives unescaped) so an
/// access code containing `/`, `#`, `?`, or whitespace still round-trips through
/// `/access-codes/{access_key}`. The same rule serves a query-string value -- see
/// `crate::link::percent_encode_component`, which owns the implementation and its tests.
fn percent_encode_segment(segment: &str) -> String {
    crate::link::percent_encode_component(segment)
}

fn bearer(builder: RequestBuilder, token: &str) -> RequestBuilder {
    builder.header("authorization", &format!("Bearer {token}"))
}

async fn body_of(response: &Response) -> String {
    response.text().await.unwrap_or_default()
}

async fn parse_json<T: serde::de::DeserializeOwned + shared::validate::WireType>(response: Response) -> Result<T, ApiError> {
    if !response.ok() {
        return Err(error::error_for(response.status(), &body_of(&response).await));
    }
    let text = body_of(&response).await;
    shared::validate::decode(&text).map_err(|error| ApiError::Malformed(error.to_string()))
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

/// `GET /rooms`, admin-only -- see `crate::rooms::RoomAdmin`, the reactive facade this backs.
/// `search` is a case-insensitive substring match on the server side; an empty string matches
/// every room. `cursor` is a previous page's own `RoomList::next_cursor`, opaque to this crate.
pub async fn rooms(token: &str, search: &str, limit: usize, cursor: Option<&str>) -> Result<RoomList, ApiError> {
    let mut path = format!("/rooms?limit={limit}");
    if !search.is_empty() {
        path.push_str(&format!("&q={}", crate::link::percent_encode_component(search)));
    }
    if let Some(cursor) = cursor {
        path.push_str(&format!("&cursor={}", crate::link::percent_encode_component(cursor)));
    }
    let request = bearer(Request::get(&endpoint(&path)), token);
    let response = request.send().await.map_err(|error| ApiError::Transport(error.to_string()))?;
    parse_json(response).await
}

/// `DELETE /rooms/{room_name}`, admin-only -- closes the room and disconnects everyone in it (see
/// `shared::protocol::LeaveReason::RoomClosed`).
pub async fn delete_room(token: &str, room_name: &str) -> Result<(), ApiError> {
    let path = format!("/rooms/{}", percent_encode_segment(room_name));
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
}
