//! The Munchkin game engine binary.
//!
//! Startup order matters: we load the config first (it holds the log file
//! path), then bring up logging, then log where the config came from, then take
//! the single-instance lock, then run the engine.

mod cli;
mod lock;
mod rules;

use anyhow::Result;
use clap::Parser;
use shared::config::Config;
use shared::logging::{self, AppMode};

use cli::Cli;

fn main() -> Result<()> {
    let cli = Cli::parse();

    // 1. Load config (logging needs the log file path from it).
    let loaded = Config::load(cli.config.as_deref())?;

    // 2. Bring up logging (engine logs to the shared file *and* stderr).
    let _log_guard = logging::init(&loaded.config, AppMode::Engine)?;

    // 3. Now that logging works, record where the config came from and its text.
    tracing::info!(source = %loaded.source, "loaded config");
    if loaded.raw.is_empty() {
        tracing::info!("no config file found; using built-in defaults");
    } else {
        tracing::info!("config contents:\n{}", loaded.raw.trim_end());
    }

    // 4. Enforce single-instance: abort if another engine is already running.
    let _instance = lock::acquire(&loaded.config.lock_file)?;
    tracing::debug!(lock = %loaded.config.lock_file.display(), "acquired single-instance lock");

    // 5. Run the (stubbed) game engine.
    rules::run()
}
