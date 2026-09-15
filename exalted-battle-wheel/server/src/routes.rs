use crate::error::ApiError;
use axum::Router;
use axum::routing::get;

pub fn router() -> Router {
    Router::new().route("/health", get(health)).fallback(not_found)
}

async fn health() -> &'static str {
    "ok"
}

async fn not_found() -> ApiError {
    ApiError::NotFound
}
