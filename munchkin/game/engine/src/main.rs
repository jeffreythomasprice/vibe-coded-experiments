//! The Munchkin game engine binary.
//!
//! Startup order matters: we load the config first (it holds the log file
//! path), then bring up logging, then log where the config came from, then take
//! the single-instance lock, open the database, and finally serve the IPC
//! socket.
//!
//! `main` stays synchronous: the database layer (`db`) owns its own
//! current-thread tokio runtime and runs queries via `block_on`, so we can't
//! wrap `main` in `#[tokio::main]` (that would panic with a nested runtime when
//! `Db::open` blocks on its own). Instead we build a dedicated runtime here and
//! `block_on` the long-running IPC server.

mod cli;
mod db;
mod lock;
mod registry;
mod rules;
mod server;

use anyhow::{Context, Result};
use clap::Parser;
use shared::config::Config;
use shared::logging::{self, AppMode};

use cli::Cli;
use db::Db;

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

    // 5. Open the database and apply any pending migrations.
    let _db = Db::open(&loaded.config.database_file)?;
    tracing::info!(db = %loaded.config.database_file.display(), "database ready");

    // 6. Run the (stubbed) game-rules init.
    rules::run()?;

    // 7. Serve the IPC socket. This runs until the process is killed; the lock,
    //    log guard, and db handle above stay alive for its whole duration.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("building IPC server runtime")?;
    rt.block_on(server::run(&loaded.config))
}
