use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use chess_shared::{AiEngineInfo, AiEnginesResponse, GameVariant};

use crate::auth::AuthUser;
use crate::AppState;

const ALL_VARIANTS: &[GameVariant] = &[
    GameVariant::Standard,
    GameVariant::ForcedCaptureLoseAll,
    GameVariant::ForcedCaptureCheckmate,
];

pub async fn list_engines(
    _auth_user: AuthUser,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let engines = state
        .ai_manager
        .registry
        .all_engines()
        .iter()
        .map(|e| AiEngineInfo {
            name: e.name().to_string(),
            description: e.description().to_string(),
            supported_variants: ALL_VARIANTS
                .iter()
                .filter(|v| e.supports_variant(v))
                .map(|v| serde_json::to_value(v).unwrap())
                .map(|v| v.as_str().unwrap().to_string())
                .collect(),
            parameters: e.parameter_defs(),
        })
        .collect();

    Json(AiEnginesResponse { engines })
}
