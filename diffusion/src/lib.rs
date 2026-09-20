pub mod catalog;
pub mod cli;
pub mod config;
pub mod error;
pub mod generate;
pub mod hub;
pub mod image_io;
pub mod llm;
pub mod log_file;
pub mod logging;
pub mod models;
pub mod output;
pub mod preset;
pub mod report;
pub mod sdlog;

use std::path::{Path, PathBuf};

use cli::{Command, GenerateArgs, ModelsCommand};
use error::AppError;

pub fn run(mut cli: cli::Cli) -> Result<Vec<PathBuf>, AppError> {
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

    match &mut cli.command {
        Command::Generate(args) => {
            preset::apply(args, &loaded.config.presets)?;
            sdlog::init();
            generate_command(args, &loaded.config.models_dir)
        }
        Command::Models { command } => {
            match command {
                ModelsCommand::Search(args) => catalog::search(args, &loaded.config.models_dir)?,
                ModelsCommand::List(args) => catalog::list(args, &loaded.config.models_dir)?,
            }
            Ok(Vec::new())
        }
    }
}

fn generate_command(args: &GenerateArgs, models_dir: &Path) -> Result<Vec<PathBuf>, AppError> {
    if args.json && args.show {
        tracing::warn!("--show writes image data to stdout, corrupting --json output");
    }
    require_checkpoint(args)?;
    match &args.output {
        Some(path) => generate_to_file(args, models_dir, path),
        None if args.show => {
            generate_to_terminal(args, models_dir)?;
            Ok(Vec::new())
        }
        None => {
            tracing::warn!("no --output path and no --show; image will not be output");
            Ok(Vec::new())
        }
    }
}

/// `--model`/`--diffusion-model` used to be a required `ArgGroup`, but a preset
/// can supply either, so the check now runs after preset merging.
fn require_checkpoint(args: &GenerateArgs) -> Result<(), AppError> {
    if args.model.is_none() && args.diffusion_model.is_none() {
        return Err(AppError::NoCheckpoint);
    }
    Ok(())
}

/// Generate `args.copies` images and write them to `path` (a plain file for one copy, a
/// filename prefix for more), then display each one, labeled, if `--show` was passed.
fn generate_to_file(
    args: &GenerateArgs,
    models_dir: &Path,
    path: &Path,
) -> Result<Vec<PathBuf>, AppError> {
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

    Ok(dests)
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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use cli::Cli;

    fn parse(args: &[&str]) -> GenerateArgs {
        let mut full = vec!["diffusion", "generate"];
        full.extend_from_slice(args);
        match Cli::try_parse_from(full).unwrap().command {
            Command::Generate(args) => *args,
            other => panic!("expected Command::Generate, got {other:?}"),
        }
    }

    #[test]
    fn model_satisfies_checkpoint() {
        let args = parse(&["a prompt", "--model", "stabilityai/sd-turbo"]);
        require_checkpoint(&args).unwrap();
    }

    #[test]
    fn diffusion_model_satisfies_checkpoint() {
        let args = parse(&["a prompt", "--diffusion-model", "stabilityai/sd-turbo"]);
        require_checkpoint(&args).unwrap();
    }

    #[test]
    fn neither_is_no_checkpoint() {
        let args = parse(&["a prompt"]);
        assert!(matches!(
            require_checkpoint(&args),
            Err(AppError::NoCheckpoint)
        ));
    }
}
