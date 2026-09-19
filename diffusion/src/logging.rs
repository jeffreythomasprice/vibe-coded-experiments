use tracing_subscriber::EnvFilter;

use crate::config::ConfigError;

pub fn init(fallback: &str) -> Result<(), ConfigError> {
    let filter = match EnvFilter::try_from_default_env() {
        Ok(filter) => filter,
        Err(_) => EnvFilter::try_new(fallback).map_err(|source| ConfigError::LogFilter {
            value: fallback.to_owned(),
            source,
        })?,
    };

    tracing_subscriber::fmt().with_env_filter(filter).init();
    Ok(())
}
