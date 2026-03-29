use axum::extract::{FromRequest, Request};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::de::DeserializeOwned;

/// Maps a request/response type to its raw JSON Schema string for validation.
pub trait HasSchema {
    fn schema() -> &'static str;
}

impl HasSchema for chess_shared::LoginRequest {
    fn schema() -> &'static str {
        chess_shared::schemas::LOGIN_REQUEST
    }
}

impl HasSchema for chess_shared::CreateUserRequest {
    fn schema() -> &'static str {
        chess_shared::schemas::CREATE_USER_REQUEST
    }
}

impl HasSchema for chess_shared::UpdateUserRequest {
    fn schema() -> &'static str {
        chess_shared::schemas::UPDATE_USER_REQUEST
    }
}

impl HasSchema for chess_shared::CreateGameRequest {
    fn schema() -> &'static str {
        chess_shared::schemas::CREATE_GAME_REQUEST
    }
}

pub struct ValidatedJson<T>(pub T);

pub enum ValidationError {
    InvalidJson(String),
    SchemaViolation(Vec<String>),
}

impl IntoResponse for ValidationError {
    fn into_response(self) -> Response {
        let (status, body) = match self {
            ValidationError::InvalidJson(msg) => (
                StatusCode::BAD_REQUEST,
                serde_json::json!({"error": "invalid_json", "message": msg}),
            ),
            ValidationError::SchemaViolation(errors) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                serde_json::json!({"error": "validation_failed", "details": errors}),
            ),
        };
        (status, axum::Json(body)).into_response()
    }
}

impl<S, T> FromRequest<S> for ValidatedJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned + HasSchema,
{
    type Rejection = ValidationError;

    async fn from_request(req: Request, _state: &S) -> Result<Self, Self::Rejection> {
        let bytes = axum::body::to_bytes(req.into_body(), 1_048_576)
            .await
            .map_err(|e| ValidationError::InvalidJson(e.to_string()))?;

        let value: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|e| ValidationError::InvalidJson(e.to_string()))?;

        let schema: serde_json::Value = serde_json::from_str(T::schema())
            .expect("embedded schema must be valid JSON");

        let validator = jsonschema::validator_for(&schema)
            .expect("embedded schema must be a valid JSON Schema");

        let errors: Vec<String> = validator
            .iter_errors(&value)
            .map(|e| e.to_string())
            .collect();

        if !errors.is_empty() {
            return Err(ValidationError::SchemaViolation(errors));
        }

        let typed: T = serde_json::from_value(value)
            .map_err(|e| ValidationError::InvalidJson(e.to_string()))?;

        Ok(ValidatedJson(typed))
    }
}
