mod serve;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use editor_proto::{Config, ConfigError};

#[derive(Parser)]
#[command(
    about = "Headless scene editor server: owns sessions and serves clients on a unix socket"
)]
struct Args {
    /// Config file (default: $EDITOR_CONFIG or ~/.config/editor/config.toml)
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
            eprintln!("editor-server: {e}");
            ExitCode::FAILURE
        }
    }
}
