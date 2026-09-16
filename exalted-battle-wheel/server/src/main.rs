mod access_codes;
mod auth;
mod config;
mod connections;
mod dynamo_client;
mod error;
mod random_key;
mod rooms;
mod routes;
mod ws;

use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use config::{Config, ConfigError, DotenvOutcome};
use routes::AppState;
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
    let state = AppState {
        access_codes: access_codes::connect(&config).await,
        rooms: rooms::connect(&config).await,
        connections: connections::connect(&config).await,
        hub: ws::Hub::default(),
    };

    // An explicit header list rather than `Any`: `Any` emits `Access-Control-Allow-Headers: *`,
    // which stops covering `Authorization` the moment credentialed requests are ever turned on.
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(config.cors_origins.clone()))
        .allow_methods(Any)
        .allow_headers([AUTHORIZATION, CONTENT_TYPE]);

    let app = routes::router(state).layer(cors).layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(config.address)
        .await
        .map_err(|source| StartupError::Bind { address: config.address, source })?;

    tracing::info!(address = %config.address, "listening");

    axum::serve(listener, app).with_graceful_shutdown(shutdown_signal()).await.map_err(StartupError::Serve)
}

/// Deliberately not `#[tokio::main]`: `server/.env` must be loaded into the process environment
/// before the runtime -- and therefore its worker threads -- exists, so nothing else can be
/// concurrently reading the environment while it's being mutated.
fn main() -> ExitCode {
    let dotenv_outcome = config::load_dotenv();

    // Read directly, ahead of `Config::from_env`, so a filter is in place to log any error that
    // call turns up.
    init_logging(&config::log_directives());
    match dotenv_outcome {
        DotenvOutcome::Loaded => tracing::info!(path = config::DOTENV_PATH, "loaded local .env"),
        DotenvOutcome::NotFound => {}
        DotenvOutcome::Unreadable(error) => tracing::warn!(%error, "could not read server/.env"),
    }
    tracing::info!("starting server");

    let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            tracing::error!(%error, "could not start the async runtime");
            return ExitCode::FAILURE;
        }
    };

    match runtime.block_on(run()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(%error, "startup failed");
            ExitCode::FAILURE
        }
    }
}
