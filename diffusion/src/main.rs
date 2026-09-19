mod cli;
mod config;
mod error;
mod generate;
mod image_io;
mod log_file;
mod logging;
mod models;
mod output;
mod sdlog;

use std::io::IsTerminal;
use std::path::{Path, PathBuf};
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
    logging::init(&loaded.config)?;
    sdlog::init();

    match &loaded.source {
        Some(path) => tracing::debug!(path = %path.display(), "loaded config"),
        None => tracing::debug!("no config file found; using defaults"),
    }
    tracing::debug!(
        models_dir = %loaded.config.models_dir.display(),
        log_dir = %loaded.config.log_dir.display(),
        "resolved config"
    );

    match &cli.output {
        Some(path) => generate_to_file(&cli, &loaded.config.models_dir, path)?,
        None if cli.show || std::io::stdout().is_terminal() => {
            generate_to_terminal(&cli, &loaded.config.models_dir)?
        }
        None => return Err(AppError::NoOutputTarget),
    }

    Ok(())
}

/// Generate `cli.copies` images and write them to `path` (a plain file for one copy, a
/// filename prefix for more), then display each one, labeled, if `--show` was passed.
fn generate_to_file(cli: &cli::Cli, models_dir: &Path, path: &Path) -> Result<(), AppError> {
    if path.is_dir() {
        return Err(AppError::OutputIsDirectory {
            path: path.to_path_buf(),
        });
    }
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }

    let dests = output::destinations(path, cli.copies);

    if cli.copies == 1 {
        generate::generate(cli, models_dir, &dests[0], 1)?;
        tracing::info!(path = %dests[0].display(), "wrote image");
    } else {
        let scratch_parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let scratch = tempfile::Builder::new()
            .prefix(".diffusion-")
            .tempdir_in(scratch_parent)?;
        generate::generate(cli, models_dir, scratch.path(), cli.copies)?;
        let sources = output::collect(scratch.path(), cli.copies)?;
        for (src, dest) in sources.iter().zip(&dests) {
            output::place(src, dest)?;
            tracing::info!(path = %dest.display(), "wrote image");
        }
    }

    if cli.show {
        display_all(&dests)?;
    }

    Ok(())
}

/// Generate `cli.copies` images into a scratch directory that is deleted on return,
/// displaying each without a path label since there is no durable file to point at.
fn generate_to_terminal(cli: &cli::Cli, models_dir: &Path) -> Result<(), AppError> {
    if cli.copies == 1 {
        let temp = tempfile::Builder::new().suffix(".png").tempfile()?;
        generate::generate(cli, models_dir, temp.path(), 1)?;
        image_io::display(temp.path())?;
        return Ok(());
    }

    let scratch = tempfile::tempdir()?;
    generate::generate(cli, models_dir, scratch.path(), cli.copies)?;
    let sources = output::collect(scratch.path(), cli.copies)?;
    for (index, src) in sources.iter().enumerate() {
        if index > 0 {
            println!();
        }
        image_io::display(src)?;
    }
    Ok(())
}

fn display_all(paths: &[PathBuf]) -> Result<(), AppError> {
    for (index, path) in paths.iter().enumerate() {
        if index > 0 {
            println!();
        }
        image_io::display_labeled(path)?;
    }
    Ok(())
}
