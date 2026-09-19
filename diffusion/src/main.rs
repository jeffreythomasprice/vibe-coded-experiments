mod cli;
mod config;
mod error;
mod generate;
mod image_io;
mod logging;
mod models;
mod sdlog;

use std::io::IsTerminal;
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
    sdlog::init();

    match &loaded.source {
        Some(path) => tracing::debug!(path = %path.display(), "loaded config"),
        None => tracing::debug!("no config file found; using defaults"),
    }
    tracing::debug!(models_dir = %loaded.config.models_dir.display(), "resolved config");

    match &cli.output {
        Some(path) => {
            if let Some(parent) = path.parent()
                && !parent.as_os_str().is_empty()
            {
                std::fs::create_dir_all(parent)?;
            }
            generate::generate(&cli, &loaded.config.models_dir, path)?;
            if cli.show {
                image_io::display(path)?;
            }
        }
        None if cli.show || std::io::stdout().is_terminal() => {
            let temp = tempfile::Builder::new().suffix(".png").tempfile()?;
            generate::generate(&cli, &loaded.config.models_dir, temp.path())?;
            image_io::display(temp.path())?;
        }
        None => return Err(AppError::NoOutputTarget),
    }

    Ok(())
}
