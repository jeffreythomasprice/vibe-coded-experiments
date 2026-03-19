mod config;
mod copy;
mod diff;
mod logging;
mod resolve;
mod summary;
mod sync;
mod tui;

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use tracing::{error, info};

use config::load_config;
use logging::init_logging;
use resolve::{resolve_entity, ResolvedEntity};
use summary::Summary;
use sync::sync_file_group;

#[derive(Parser, Debug)]
#[command(name = "file-sync", about = "Synchronize files across multiple locations")]
struct Cli {
    /// Path to config file
    #[arg(long)]
    config: Option<PathBuf>,

    /// Directory for log files
    #[arg(long)]
    log_dir: Option<PathBuf>,

    /// Only sync specific entities by name
    #[arg(long)]
    entity: Vec<String>,

    /// Show what would happen without making changes
    #[arg(long)]
    dry_run: bool,

    /// Print status for every file, including those already in sync
    #[arg(long, short)]
    verbose: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let config = load_config(cli.config.as_deref())?;

    let log_dir = cli
        .log_dir
        .or(config.log_dir.clone())
        .unwrap_or_else(|| PathBuf::from("/tmp/file-sync/logs"));

    let _guard = init_logging(&log_dir)?;

    info!("file-sync starting");

    if cli.dry_run {
        info!("Running in dry-run mode");
    }

    let mut summary = Summary::new();

    let entities: Vec<_> = if cli.entity.is_empty() {
        config.sync.iter().collect()
    } else {
        config
            .sync
            .iter()
            .filter(|e| cli.entity.contains(&e.name))
            .collect()
    };

    if entities.is_empty() {
        println!("No matching sync entities found.");
        return Ok(());
    }

    for entity in &entities {
        info!("Processing entity: {}", entity.name);
        if cli.verbose {
            println!("[{}]", entity.name);
        }

        let resolved = match resolve_entity(entity) {
            Ok(r) => r,
            Err(e) => {
                error!("Failed to resolve entity '{}': {}", entity.name, e);
                summary.record_error(format!("Entity '{}': {}", entity.name, e));
                continue;
            }
        };

        match resolved {
            ResolvedEntity::Files { groups } => {
                for group in &groups {
                    match sync_file_group(&entity.name, group, cli.dry_run, cli.verbose, &mut summary) {
                        Ok(()) => {}
                        Err(e) => {
                            let msg = format!("{}", e);
                            if msg == "User quit" {
                                summary.print();
                                return Ok(());
                            }
                            error!(
                                "Error syncing '{}' in entity '{}': {}",
                                group.relative_name, entity.name, e
                            );
                            summary.record_error(format!(
                                "{}/{}: {}",
                                entity.name, group.relative_name, e
                            ));
                        }
                    }
                }
            }
        }
    }

    summary.print();

    info!("file-sync complete");
    Ok(())
}
