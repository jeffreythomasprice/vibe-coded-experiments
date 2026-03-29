use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use crate::AppState;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub username: String,
    pub is_admin: bool,
    pub exp: i64,
    pub iat: i64,
}

#[derive(Debug)]
pub struct AuthUser {
    pub user_id: String,
    pub username: String,
    pub is_admin: bool,
}

pub enum AuthError {
    MissingToken,
    InvalidToken,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let msg = match self {
            AuthError::MissingToken => "missing authorization token",
            AuthError::InvalidToken => "invalid or expired token",
        };
        (
            StatusCode::UNAUTHORIZED,
            axum::Json(serde_json::json!({"error": msg})),
        )
            .into_response()
    }
}

pub fn create_token(secret: &str, user_id: &str, username: &str, is_admin: bool) -> Result<String, jsonwebtoken::errors::Error> {
    let now = Utc::now().timestamp();
    let claims = Claims {
        sub: user_id.to_string(),
        username: username.to_string(),
        is_admin,
        iat: now,
        exp: now + 86400, // 24 hours
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let auth_header = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .ok_or(AuthError::MissingToken)?;

        let token = auth_header
            .strip_prefix("Bearer ")
            .ok_or(AuthError::MissingToken)?;

        let token_data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(state.jwt_secret.as_bytes()),
            &Validation::default(),
        )
        .map_err(|_| AuthError::InvalidToken)?;

        Ok(AuthUser {
            user_id: token_data.claims.sub,
            username: token_data.claims.username,
            is_admin: token_data.claims.is_admin,
        })
    }
}
