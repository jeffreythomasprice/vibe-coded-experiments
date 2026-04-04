use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use chess_shared::Theme;

use crate::auth::AuthUser;
use crate::extractors::ValidatedJson;
use crate::AppState;

fn default_classic_theme() -> serde_json::Value {
    serde_json::json!({
        "name": "Classic",
        "board": {
            "light_square": "#f0d9b5",
            "dark_square": "#b58863"
        },
        "pieces": {
            "mode": "circle",
            "white_fill": "#ffffff",
            "black_fill": "#333333",
            "white_text": "#333333",
            "black_text": "#ffffff"
        }
    })
}

pub async fn get_my_theme(
    auth_user: AuthUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let row = sqlx::query_as::<_, (Option<serde_json::Value>,)>(
        "SELECT theme FROM users WHERE id = $1",
    )
    .bind(auth_user.user_id.parse::<uuid::Uuid>().unwrap())
    .fetch_optional(&state.db)
    .await
    .map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "internal_error"})),
        )
    })?;

    let theme = row
        .and_then(|r| r.0)
        .unwrap_or_else(default_classic_theme);

    Ok((StatusCode::OK, Json(theme)))
}

pub async fn update_my_theme(
    auth_user: AuthUser,
    State(state): State<AppState>,
    ValidatedJson(theme): ValidatedJson<Theme>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let theme_json = serde_json::to_value(&theme).map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "internal_error"})),
        )
    })?;

    sqlx::query("UPDATE users SET theme = $1 WHERE id = $2")
        .bind(&theme_json)
        .bind(auth_user.user_id.parse::<uuid::Uuid>().unwrap())
        .execute(&state.db)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal_error"})),
            )
        })?;

    Ok((StatusCode::OK, Json(theme_json)))
}
