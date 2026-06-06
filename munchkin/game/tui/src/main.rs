//! The Munchkin terminal UI binary.
//!
//! Startup mirrors the engine (load config, then logging, then log provenance),
//! but the tui takes no single-instance lock and does not mirror logs to stderr
//! — it owns the terminal, so logs go to the shared file only. It then connects
//! to the engine's IPC socket (exiting non-zero if the engine isn't running).
//!
//! The UI itself is a stub for now; a terminal-UI framework (e.g. ratatui +
//! crossterm) will be added later. `main` is async (single-threaded runtime) so
//! it can drive that one engine connection.

mod cli;
mod session;

use anyhow::Result;
use clap::Parser;
use shared::config::Config;
use shared::logging::{self, AppMode};

use cli::Cli;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // 1. Load config (logging needs the log file path from it).
    let loaded = Config::load(cli.config.as_deref())?;

    // 2. Bring up logging (tui logs to the shared file only).
    let _log_guard = logging::init(&loaded.config, AppMode::Tui)?;

    // 3. Record where the config came from and its text.
    tracing::info!(source = %loaded.source, "loaded config");
    if loaded.raw.is_empty() {
        tracing::info!("no config file found; using built-in defaults");
    } else {
        tracing::info!("config contents:\n{}", loaded.raw.trim_end());
    }

    // 4. Connect to the engine and run the (stubbed) session.
    session::run(&loaded.config).await
}
