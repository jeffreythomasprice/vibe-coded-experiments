use std::{
    fs::{File, OpenOptions},
    io,
    path::{Path, PathBuf},
};

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, Layer, layer::SubscriberExt, util::SubscriberInitExt};

use crate::{ResolvedConfig, paths};

#[derive(Debug, thiserror::Error)]
pub enum LoggingError {
    #[error("failed to create log directory {}: {source}", path.display())]
    CreateDir { path: PathBuf, source: io::Error },
    #[error("failed to open log file {}: {source}", path.display())]
    Open { path: PathBuf, source: io::Error },
}

pub struct LogGuard(#[allow(dead_code)] Option<WorkerGuard>);

pub fn open_file(path: &Path) -> Result<File, LoggingError> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .map_err(|source| LoggingError::CreateDir { path: parent.to_path_buf(), source })?;
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|source| LoggingError::Open { path: path.to_path_buf(), source })
}

pub fn init(file: Option<File>) -> LogGuard {
    let filter = || EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let stderr = tracing_subscriber::fmt::layer().with_writer(io::stderr).with_filter(filter());
    let (file_layer, guard) = match file {
        Some(file) => {
            let (writer, guard) = tracing_appender::non_blocking(file);
            let layer = tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(writer)
                .with_filter(filter());
            (Some(layer), Some(guard))
        }
        None => (None, None),
    };
    tracing_subscriber::registry().with(stderr).with(file_layer).init();
    LogGuard(guard)
}

pub fn init_from(config: Option<&ResolvedConfig>) -> LogGuard {
    let path = config.map(|c| c.log_file.clone()).unwrap_or_else(paths::default_log_file);
    match open_file(&path) {
        Ok(file) => init(Some(file)),
        Err(e) => {
            let guard = init(None);
            tracing::warn!("{e}; logging to stderr only");
            guard
        }
    }
}
