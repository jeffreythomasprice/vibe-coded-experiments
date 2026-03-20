mod config;
mod copy;
mod diff;
mod logging;
mod resolve;
mod summary;
mod sync;
mod tui;

use std::path::PathBuf;
use std::time::SystemTime;

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

    /// Directory for backup files
    #[arg(long)]
    backup_dir: Option<PathBuf>,

    /// Print status for every file, including those already in sync
    #[arg(long, short)]
    verbose: bool,
}

fn format_timestamp(time: SystemTime) -> String {
    let dur = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs();

    // Calculate UTC date/time components
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Days since epoch to year/month/day
    let mut y = 1970i64;
    let mut remaining_days = days as i64;
    loop {
        let year_days = if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) {
            366
        } else {
            365
        };
        if remaining_days < year_days {
            break;
        }
        remaining_days -= year_days;
        y += 1;
    }
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let month_days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut m = 0usize;
    for md in &month_days {
        if remaining_days < *md as i64 {
            break;
        }
        remaining_days -= *md as i64;
        m += 1;
    }

    format!(
        "{:04}-{:02}-{:02}T{:02}-{:02}-{:02}Z",
        y,
        m + 1,
        remaining_days + 1,
        hours,
        minutes,
        seconds
    )
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

    let backup_base = cli
        .backup_dir
        .or(config.backup_dir.clone())
        .unwrap_or_else(|| PathBuf::from("/tmp/file-sync/backups"));
    let timestamp = format_timestamp(SystemTime::now());
    let backup_dir = backup_base.join(&timestamp);

    info!("Backup directory: {}", backup_dir.display());

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
                    match sync_file_group(&entity.name, group, cli.dry_run, cli.verbose, &backup_dir, &mut summary) {
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
