//! The Munchkin terminal UI binary.
//!
//! Startup mirrors the engine (load config, then logging, then log provenance),
//! but the tui takes no single-instance lock and does not mirror logs to stderr
//! — it owns the terminal, so logs go to the shared file only. It connects to
//! the engine's IPC socket (exiting non-zero if the engine isn't running), then
//! takes over the terminal with ratatui and runs the status UI until the user
//! presses Esc.
//!
//! `main` is async (single-threaded runtime) so it can drive the one engine
//! connection alongside the async terminal event stream.

mod app;
mod cli;
mod session;
mod ui;

use anyhow::Result;
use clap::Parser;
use ratatui::DefaultTerminal;
use shared::config::Config;
use shared::logging::{self, AppMode};

use app::App;
use cli::Cli;

/// Restores the terminal (leaves raw mode + the alternate screen) on every exit
/// path — normal quit, a propagated error, or a panic — via `Drop`. ratatui's
/// `init` also installs a panic hook that restores; the double restore is
/// idempotent.
struct TerminalGuard(DefaultTerminal);

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        ratatui::restore();
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // 1. Load config (logging needs the log file path from it).
    let loaded = Config::load(cli.config.as_deref())?;

    // 2. Bring up logging (tui logs to the shared file only — it owns the
    //    terminal, so anything written to stderr would corrupt the UI).
    let _log_guard = logging::init(&loaded.config, AppMode::Tui)?;

    // 3. Record where the config came from and its text.
    tracing::info!(source = %loaded.source, "loaded config");
    if loaded.raw.is_empty() {
        tracing::info!("no config file found; using built-in defaults");
    } else {
        tracing::info!("config contents:\n{}", loaded.raw.trim_end());
    }

    // 4. Connect + handshake *before* taking over the terminal, so a missing
    //    engine fails fast with a normal error message (and non-zero exit).
    let (mut client, server) = session::connect(&loaded.config).await?;
    let mut state = App::new(server);

    // 5. Take over the terminal and run the UI. The guard restores it on the
    //    way out regardless of how `run_ui` returns.
    let mut guard = TerminalGuard(ratatui::init());
    let result = app::run_ui(&mut guard.0, &mut client, &mut state).await;
    drop(guard); // restore before any error is printed to stderr

    result
}
