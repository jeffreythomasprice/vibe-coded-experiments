use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::Layer;
use tracing_subscriber::fmt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::config::project::LoggingConfig;

fn default_filter(caller_crate: &str) -> String {
    if caller_crate == "robowar_shared" {
        "warn,robowar_shared=trace".to_string()
    } else {
        format!("warn,robowar_shared=trace,{caller_crate}=trace")
    }
}

fn make_filter(caller_crate: &str) -> EnvFilter {
    EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(default_filter(caller_crate)))
}

pub fn init_logging(
    log_dir: Option<&Path>,
    no_file_log: bool,
    config: &LoggingConfig,
    caller_crate: &str,
) -> Result<Option<PathBuf>> {
    let file_logging = !no_file_log && config.file_logging;

    let log_file = if file_logging {
        try_setup_log_file(log_dir, config)
    } else {
        None
    };

    let stdout_layer = fmt::layer()
        .with_target(true)
        .with_filter(make_filter(caller_crate));

    match log_file {
        Some((file, log_path)) => {
            let file_layer = fmt::layer()
                .with_writer(file)
                .with_ansi(false)
                .with_target(true)
                .with_filter(make_filter(caller_crate));

            tracing_subscriber::registry()
                .with(stdout_layer)
                .with(file_layer)
                .init();

            Ok(Some(log_path))
        }
        None => {
            tracing_subscriber::registry().with(stdout_layer).init();
            Ok(None)
        }
    }
}

fn try_setup_log_file(
    log_dir: Option<&Path>,
    config: &LoggingConfig,
) -> Option<(fs::File, PathBuf)> {
    let dir = log_dir
        .map(|p| p.to_path_buf())
        .or_else(|| config.log_dir.clone())
        .or_else(|| dirs::data_dir().map(|d| d.join("robowar").join("logs")))?;

    if let Err(e) = fs::create_dir_all(&dir) {
        eprintln!(
            "warn: failed to create log directory '{}': {e}, file logging disabled",
            dir.display()
        );
        return None;
    }

    let log_path = dir.join("robowar.log");

    match fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    {
        Ok(file) => Some((file, log_path)),
        Err(e) => {
            eprintln!(
                "warn: failed to create log file '{}': {e}, file logging disabled",
                log_path.display()
            );
            None
        }
    }
}
