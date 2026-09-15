mod config;
mod error;
mod routes;

use config::{Config, ConfigError};
use std::net::SocketAddr;
use std::process::ExitCode;
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

#[derive(Debug, thiserror::Error)]
enum StartupError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error("could not bind {address}: {source}")]
    Bind { address: SocketAddr, source: std::io::Error },
    #[error("server error: {0}")]
    Serve(#[source] std::io::Error),
}

fn init_logging(directives: &str) {
    let filter = EnvFilter::new(directives);
    let fmt_layer = tracing_subscriber::fmt::layer();
    tracing_subscriber::registry().with(filter).with(fmt_layer).init();
}

/// Waits for either Ctrl-C or SIGTERM — kubelet sends the latter on every rollout, and without a
/// handler for it the pod eats the full termination grace period on every deploy.
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => tracing::warn!(%error, "could not install SIGTERM handler"),
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }

    tracing::info!("shutdown signal received");
}

async fn run() -> Result<(), StartupError> {
    let config = Config::from_env()?;

    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(config.cors_origins.clone()))
        .allow_methods(Any)
        .allow_headers(Any);

    let app = routes::router().layer(cors).layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(config.address)
        .await
        .map_err(|source| StartupError::Bind { address: config.address, source })?;

    tracing::info!(address = %config.address, "listening");

    axum::serve(listener, app).with_graceful_shutdown(shutdown_signal()).await.map_err(StartupError::Serve)
}

#[tokio::main]
async fn main() -> ExitCode {
    // Read directly, ahead of `Config::from_env`, so a filter is in place to log any error that
    // call turns up.
    init_logging(&config::log_directives());
    tracing::info!("starting server");

    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(%error, "startup failed");
            ExitCode::FAILURE
        }
    }
}
