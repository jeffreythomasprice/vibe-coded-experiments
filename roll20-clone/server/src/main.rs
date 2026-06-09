mod config;
mod http;
mod state;
mod ws;

use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    // Load `.env` if present (does nothing if the file is missing).
    dotenvy::dotenv().ok();
    init_tracing();

    let config = config::Config::from_env();
    let state = state::AppState::new();

    let app = http::router(state)
        // The Leptos client is served from a different origin (port 8000) in
        // dev, so allow cross-origin requests.
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    let addr = config.socket_addr();
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| panic!("failed to bind {addr}: {e}"));

    tracing::info!(%addr, "server listening");
    axum::serve(listener, app)
        .await
        .expect("server error");
}

/// Initialize `tracing`: our crates at TRACE, everything else at WARN.
/// Overridable via the `RUST_LOG` environment variable.
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("warn,server=trace,shared=trace"));

    tracing_subscriber::fmt().with_env_filter(filter).init();
}
