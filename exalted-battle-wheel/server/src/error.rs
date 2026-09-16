use crate::access_codes::StoreError;
use crate::rooms::RoomStoreError;
use aws_sdk_dynamodb::error::DisplayErrorContext;
use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use shared::access::ApiErrorBody;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("not found")]
    NotFound,
    #[error("missing or unknown access code")]
    Unauthorized,
    #[error("access code is not an admin")]
    Forbidden,
    #[error("cannot modify or delete your own access code")]
    SelfModification,
    #[error("access code already exists")]
    Conflict,
    // Deliberately opaque: a `StoreError` can carry a DynamoDB error with the table name, endpoint,
    // and request id in it. `to_string()` is what a caller sees; the real cause only reaches the log.
    #[error("internal error")]
    Internal(#[source] StoreError),
    // Same reasoning as `Internal`, for the one HTTP handler (`GET /rooms`) that reads the room
    // store directly -- everything else that touches rooms goes through the websocket, which maps
    // `RoomStoreError` to `shared::protocol::ProtocolError` instead (see `ws::handler::store_error`).
    #[error("internal error")]
    RoomStore(#[source] RoomStoreError),
}

impl ApiError {
    fn status(&self) -> StatusCode {
        match self {
            ApiError::NotFound => StatusCode::NOT_FOUND,
            ApiError::Unauthorized => StatusCode::UNAUTHORIZED,
            ApiError::Forbidden | ApiError::SelfModification => StatusCode::FORBIDDEN,
            ApiError::Conflict => StatusCode::CONFLICT,
            ApiError::Internal(_) | ApiError::RoomStore(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl From<StoreError> for ApiError {
    fn from(error: StoreError) -> Self {
        match error {
            StoreError::NotFound => ApiError::NotFound,
            StoreError::AlreadyExists => ApiError::Conflict,
            error => ApiError::Internal(error),
        }
    }
}

impl From<RoomStoreError> for ApiError {
    fn from(error: RoomStoreError) -> Self {
        ApiError::RoomStore(error)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        // `SdkError`'s own `Display` is just "service error" -- `DisplayErrorContext` walks the
        // source chain to the actual DynamoDB error, so the log line is worth reading.
        match &self {
            ApiError::Internal(source) => tracing::error!(error = %DisplayErrorContext(source), "request failed"),
            ApiError::RoomStore(source) => tracing::error!(error = %DisplayErrorContext(source), "request failed"),
            _ => {}
        }

        let status = self.status();
        (status, Json(ApiErrorBody { error: self.to_string() })).into_response()
    }
}
