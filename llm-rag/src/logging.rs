use std::path::{Path, PathBuf};

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::fmt::time::UtcTime;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

#[derive(thiserror::Error, Debug)]
pub enum LoggingError {
    #[error("failed to create log dir {dir}: {source}")]
    CreateDir {
        dir: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to install tracing subscriber: {0}")]
    Install(String),
}

/// Initialize the file-based tracing subscriber.
///
/// Returns a `WorkerGuard` that must be held for the lifetime of the process;
/// dropping it flushes any buffered log lines from the non-blocking appender.
pub fn init(log_dir: &Path) -> Result<WorkerGuard, LoggingError> {
    std::fs::create_dir_all(log_dir).map_err(|source| LoggingError::CreateDir {
        dir: log_dir.to_path_buf(),
        source,
    })?;

    let appender = tracing_appender::rolling::never(log_dir, "llm-rag.log");
    let (nb, guard) = tracing_appender::non_blocking(appender);

    // Crate name with `-` normalized to `_` matches the Rust module-path
    // identifier that `target` filtering uses.
    let crate_target = env!("CARGO_PKG_NAME").replace('-', "_");
    let default_filter = format!("warn,{crate_target}=trace");
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter));

    let layer = fmt::layer()
        .json()
        .with_timer(UtcTime::rfc_3339())
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_current_span(true)
        .with_span_list(false)
        .with_writer(nb);

    tracing_subscriber::registry()
        .with(filter)
        .with(layer)
        .try_init()
        .map_err(|e| LoggingError::Install(e.to_string()))?;

    Ok(guard)
}
