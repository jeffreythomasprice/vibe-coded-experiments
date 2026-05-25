//! Tracing initialization. Logs go to both stderr and the configured log file.
//!
//! Default filter is `ecs=trace,exalted=trace,warn` — meaning our own crates
//! log at TRACE while every other dependency stays at WARN. Override with
//! `RUST_LOG`.

use std::fs;
use std::path::Path;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

const DEFAULT_FILTER: &str = "ecs=trace,exalted=trace,warn";

/// Initialize the global tracing subscriber. The returned `WorkerGuard` must
/// be held for the lifetime of the process — when it drops, the background
/// file-writer thread flushes and exits.
pub fn init(log_file: &Path) -> Option<WorkerGuard> {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER));

    let stderr_layer = fmt::layer().with_writer(std::io::stderr).with_ansi(true);

    let (file_layer, guard) = match build_file_writer(log_file) {
        Some((nb, guard)) => {
            let layer = fmt::layer().with_writer(nb).with_ansi(false);
            (Some(layer), Some(guard))
        }
        None => (None, None),
    };

    tracing_subscriber::registry()
        .with(env_filter)
        .with(stderr_layer)
        .with(file_layer)
        .init();

    guard
}

fn build_file_writer(
    log_file: &Path,
) -> Option<(tracing_appender::non_blocking::NonBlocking, WorkerGuard)> {
    let parent = log_file.parent().unwrap_or_else(|| Path::new("."));
    if let Err(e) = fs::create_dir_all(parent) {
        eprintln!(
            "warning: could not create log directory {}: {} (file logging disabled)",
            parent.display(),
            e
        );
        return None;
    }
    let filename = log_file.file_name()?.to_owned();
    let appender = tracing_appender::rolling::never(parent, filename);
    Some(tracing_appender::non_blocking(appender))
}
