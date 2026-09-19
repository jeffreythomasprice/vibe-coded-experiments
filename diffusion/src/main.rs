mod cli;
mod config;
mod error;
mod generate;
mod image_io;
mod logging;
mod models;

use std::process::ExitCode;

use clap::Parser;
use error::AppError;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), AppError> {
    let cli = cli::Cli::parse();
    let loaded = config::load(cli.config.as_deref())?;
    logging::init(&loaded.config.log_filter)?;

    match &loaded.source {
        Some(path) => tracing::debug!(path = %path.display(), "loaded config"),
        None => tracing::debug!("no config file found; using defaults"),
    }
    tracing::debug!(models_dir = %loaded.config.models_dir.display(), "resolved config");

    tracing::info!("diffusion scaffold initialized");
    Ok(())
}
