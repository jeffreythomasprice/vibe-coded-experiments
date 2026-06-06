//! Command-line interface for the engine binary.

use std::path::PathBuf;

use clap::Parser;

/// The Munchkin game engine.
#[derive(Debug, Parser)]
#[command(name = "engine", version, about)]
pub struct Cli {
    /// Path to a config file. If omitted, falls back to ./config.toml then
    /// ~/.config/munchkin/config.toml, then built-in defaults.
    #[arg(long, value_name = "PATH")]
    pub config: Option<PathBuf>,
}
