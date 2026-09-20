use std::path::PathBuf;

use thiserror::Error;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, fmt};

use crate::config::Config;
use crate::log_file::FileWriter;

#[derive(Debug, Error)]
pub enum LogError {
    #[error("invalid log filter {value:?}: {source}")]
    Filter {
        value: String,
        #[source]
        source: tracing_subscriber::filter::ParseError,
    },

    #[error("failed to create log directory {}: {source}", .path.display())]
    CreateDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to open log file {}: {source}", .path.display())]
    OpenFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("{key} must be at least 1")]
    InvalidLimit { key: &'static str },
}

pub fn init(config: &Config) -> Result<(), LogError> {
    let filter = match EnvFilter::try_from_default_env() {
        Ok(filter) => filter,
        Err(_) => EnvFilter::try_new(&config.log_filter).map_err(|source| LogError::Filter {
            value: config.log_filter.clone(),
            source,
        })?,
    };
    let file = FileWriter::new(config)?;

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(std::io::stderr))
        .with(fmt::layer().with_ansi(false).with_writer(file))
        .init();

    Ok(())
}
