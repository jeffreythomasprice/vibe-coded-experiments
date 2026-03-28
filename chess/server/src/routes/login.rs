use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use chess_shared::LoginRequest;

use crate::auth;
use crate::extractors::ValidatedJson;
use crate::AppState;

pub async fn login(
    State(state): State<AppState>,
    ValidatedJson(payload): ValidatedJson<LoginRequest>,
) -> Result<impl IntoResponse, impl IntoResponse> {
    let row = sqlx::query_as::<_, (String, String)>(
        "SELECT id::text, password FROM users WHERE username = $1",
    )
    .bind(payload.username.as_str())
    .fetch_optional(&state.db)
    .await
    .map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "internal_error"})),
        )
    })?;

    let (user_id, stored_password) = row.ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "invalid_credentials"})),
        )
    })?;

    if *payload.password != stored_password {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "invalid_credentials"})),
        ));
    }

    let token = auth::create_token(&state.jwt_secret, &user_id, &payload.username)
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal_error"})),
            )
        })?;

    Ok((
        StatusCode::OK,
        Json(chess_shared::LoginResponse { token }),
    ))
}
