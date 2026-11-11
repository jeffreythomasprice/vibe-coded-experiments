mod serve;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use level_proto::{Config, ConfigError};

#[derive(Parser)]
#[command(about = "Headless level editor server: owns sessions and serves clients on a unix socket")]
struct Args {
    /// Config file (default: $LEVEL_EDITOR_CONFIG or ~/.config/level-editor/config.toml)
    #[arg(long)]
    config: Option<PathBuf>,
}

#[derive(Debug, thiserror::Error)]
enum MainError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Serve(#[from] serve::ServeError),
}

fn run(args: Args) -> Result<(), MainError> {
    let config = match &args.config {
        Some(path) => Config::load_from(path)?,
        None => Config::load()?,
    };
    serve::run(&config)?;
    Ok(())
}

fn main() -> ExitCode {
    match run(Args::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("level-server: {e}");
            ExitCode::FAILURE
        }
    }
}
