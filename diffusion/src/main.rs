mod catalog;
mod cli;
mod config;
mod error;
mod generate;
mod hub;
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
use cli::{Command, GenerateArgs, ModelsCommand};
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

    match &loaded.source {
        Some(path) => tracing::debug!(path = %path.display(), "loaded config"),
        None => tracing::debug!("no config file found; using defaults"),
    }
    tracing::debug!(
        models_dir = %loaded.config.models_dir.display(),
        log_dir = %loaded.config.log_dir.display(),
        "resolved config"
    );

    match &cli.command {
        Command::Generate(args) => {
            sdlog::init();
            generate_command(args, &loaded.config.models_dir)
        }
        Command::Models { command } => match command {
            ModelsCommand::Search(args) => catalog::search(args, &loaded.config.models_dir),
            ModelsCommand::List(args) => catalog::list(args, &loaded.config.models_dir),
        },
    }
}

fn generate_command(args: &GenerateArgs, models_dir: &Path) -> Result<(), AppError> {
    match &args.output {
        Some(path) => generate_to_file(args, models_dir, path)?,
        None if args.show || std::io::stdout().is_terminal() => {
            generate_to_terminal(args, models_dir)?
        }
        None => return Err(AppError::NoOutputTarget),
    }
    Ok(())
}

/// Generate `args.copies` images and write them to `path` (a plain file for one copy, a
/// filename prefix for more), then display each one, labeled, if `--show` was passed.
fn generate_to_file(args: &GenerateArgs, models_dir: &Path, path: &Path) -> Result<(), AppError> {
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

    let dests = output::destinations(path, args.copies);

    if args.copies == 1 {
        generate::generate(args, models_dir, &dests[0], 1)?;
        tracing::info!(path = %dests[0].display(), "wrote image");
    } else {
        let scratch_parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let scratch = tempfile::Builder::new()
            .prefix(".diffusion-")
            .tempdir_in(scratch_parent)?;
        generate::generate(args, models_dir, scratch.path(), args.copies)?;
        let sources = output::collect(scratch.path(), args.copies)?;
        for (src, dest) in sources.iter().zip(&dests) {
            output::place(src, dest)?;
            tracing::info!(path = %dest.display(), "wrote image");
        }
    }

    if args.show {
        display_all(&dests)?;
    }

    Ok(())
}

/// Generate `args.copies` images into a scratch directory that is deleted on return,
/// displaying each without a path label since there is no durable file to point at.
fn generate_to_terminal(args: &GenerateArgs, models_dir: &Path) -> Result<(), AppError> {
    if args.copies == 1 {
        let temp = tempfile::Builder::new().suffix(".png").tempfile()?;
        generate::generate(args, models_dir, temp.path(), 1)?;
        image_io::display(temp.path())?;
        return Ok(());
    }

    let scratch = tempfile::tempdir()?;
    generate::generate(args, models_dir, scratch.path(), args.copies)?;
    let sources = output::collect(scratch.path(), args.copies)?;
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
