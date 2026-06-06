//! Command-line interface for the tui binary.

use std::path::PathBuf;

use clap::Parser;

/// The Munchkin terminal UI.
#[derive(Debug, Parser)]
#[command(name = "tui", version, about)]
pub struct Cli {
    /// Path to a config file. If omitted, falls back to ./config.toml then
    /// ~/.config/munchkin/config.toml, then built-in defaults.
    #[arg(long, value_name = "PATH")]
    pub config: Option<PathBuf>,
}
