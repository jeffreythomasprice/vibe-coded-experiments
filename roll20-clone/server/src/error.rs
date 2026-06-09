//! A small error type for HTTP handlers that converts into an appropriate
//! status code + JSON body.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

/// Errors returned by the JSON API handlers.
pub enum ApiError {
    /// The requested resource does not exist (404).
    NotFound,
    /// The request was malformed or semantically invalid (400).
    #[allow(dead_code)] // part of the API surface; not yet produced by a handler
    BadRequest(String),
    /// An unexpected internal error (500). Logged; not surfaced verbatim.
    Internal(anyhow::Error),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApiError::NotFound => (StatusCode::NOT_FOUND, "not found".to_string()),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            ApiError::Internal(e) => {
                tracing::error!(error = ?e, "internal server error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal server error".to_string(),
                )
            }
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        ApiError::Internal(e)
    }
}
