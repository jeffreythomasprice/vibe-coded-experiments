use axum::{extract::State, routing::get, Json, Router};
use shared::{HealthResponse, VersionResponse};

use crate::{state::AppState, ws};

/// Build the application router with all HTTP + WebSocket routes.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/version", get(version))
        .route("/ws", get(ws::handler))
        .with_state(state)
}

async fn health() -> Json<HealthResponse> {
    tracing::trace!("health check");
    Json(HealthResponse {
        status: "ok".to_string(),
    })
}

async fn version(State(_state): State<AppState>) -> Json<VersionResponse> {
    Json(VersionResponse {
        name: env!("CARGO_PKG_NAME").to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}
