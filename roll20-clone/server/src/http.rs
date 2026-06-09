use axum::{
    extract::State,
    routing::{get, post, put},
    Json, Router,
};
use shared::{HealthResponse, VersionResponse};

use crate::{maps_api, state::AppState, ws};

/// Build the application router with all HTTP + WebSocket routes.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/version", get(version))
        .route(
            "/api/maps",
            get(maps_api::list_maps).post(maps_api::create_map),
        )
        .route(
            "/api/maps/{id}",
            get(maps_api::get_map)
                .put(maps_api::update_map)
                .delete(maps_api::delete_map),
        )
        .route("/api/maps/{id}/shapes", post(maps_api::add_shape))
        .route(
            "/api/maps/{id}/shapes/{shape_id}",
            put(maps_api::update_shape).delete(maps_api::delete_shape),
        )
        .route("/api/maps/{id}/groups", post(maps_api::add_group))
        .route(
            "/api/maps/{id}/groups/{group_id}",
            put(maps_api::update_group).delete(maps_api::delete_group),
        )
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
