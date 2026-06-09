mod config;
mod db;
mod http;
mod lockfile;
mod logging;
mod state;
mod ws;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::Context;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::config::Config;
use crate::lockfile::InstanceLock;

/// Directory for runtime files (lock file). Lives under `/tmp` per the spec.
const RUNTIME_DIR: &str = "/tmp/roll20-clone";

/// Exit code used when another instance already holds the single-instance lock.
const EXIT_ALREADY_RUNNING: u8 = 3;

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(StartupError::AlreadyRunning) => {
            eprintln!("another roll20-clone server is already running");
            ExitCode::from(EXIT_ALREADY_RUNNING)
        }
        Err(StartupError::Other(e)) => {
            // `tracing` may or may not be initialized yet, so also print to stderr.
            eprintln!("fatal: {e:#}");
            ExitCode::FAILURE
        }
    }
}

enum StartupError {
    AlreadyRunning,
    Other(anyhow::Error),
}

impl From<anyhow::Error> for StartupError {
    fn from(e: anyhow::Error) -> Self {
        StartupError::Other(e)
    }
}

async fn run() -> Result<(), StartupError> {
    // 1. Load configuration (optional config path may be the first CLI arg).
    let cli_path = std::env::args().nth(1).map(PathBuf::from);
    let (mut config, source) = Config::load(cli_path)?;
    config.expand_paths()?;

    // 2. Ensure runtime directories exist.
    std::fs::create_dir_all(RUNTIME_DIR)
        .with_context(|| format!("creating runtime directory {RUNTIME_DIR}"))?;
    std::fs::create_dir_all(&config.log)
        .with_context(|| format!("creating log directory {}", config.log.display()))?;

    // 3. Enforce a single running instance before touching shared resources.
    let lock_path = Path::new(RUNTIME_DIR).join("server.lock");
    let _lock = match InstanceLock::acquire(&lock_path)
        .with_context(|| format!("acquiring lock {}", lock_path.display()))?
    {
        Some(lock) => lock,
        None => return Err(StartupError::AlreadyRunning),
    };

    // 4. Initialize logging (stderr + rotating file). Hold the guard until exit.
    let _log_guard = logging::init(&config.log)?;
    match &source {
        Some(path) => tracing::info!(config = %path.display(), "loaded config"),
        None => tracing::warn!("no config file found; using built-in defaults"),
    }

    // 5. Open the database and run migrations.
    let db = db::open(&config.db).await?;

    // 6. Build shared state and the router.
    let state = state::AppState::new(db);
    let app = http::router(state)
        // The Leptos client is served from a different origin (port 8000) in
        // dev, so allow cross-origin requests.
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    // 7. Bind and serve.
    let addr = config.socket_addr();
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding {addr}"))?;

    tracing::info!(%addr, "server listening");
    axum::serve(listener, app).await.context("server error")?;

    Ok(())
}
