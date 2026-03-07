use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use robowar_shared::config::cli_args::{LogArgs, MatchArgs};
use robowar_shared::config::constants::SimConstants;
use robowar_shared::config::loadout::RobotConfig;
use robowar_shared::config::logging::init_logging;
use robowar_shared::config::project::ProjectConfig;
use robowar_shared::config::resolve::resolve_match_config;
use robowar_shared::sim::match_runner::run_match;
use robowar_shared::vm::assembler::assemble;

#[derive(Parser)]
#[command(name = "robowar", about = "Robot arena combat simulator")]
struct Cli {
    #[command(flatten)]
    log_args: LogArgs,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run a match between robots
    Run {
        #[command(flatten)]
        args: MatchArgs,
    },

    /// Assemble a BotASM program and report errors
    Assemble {
        /// Path to the .asm file
        program: PathBuf,
    },

    /// Show robot config and computed stats
    Info {
        /// Path to the robot TOML file
        robot: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let project = match &cli.command {
        Command::Run { args } => ProjectConfig::load(&args.config)?,
        _ => ProjectConfig::default(),
    };

    init_logging(
        cli.log_args.log_dir.as_deref(),
        cli.log_args.no_file_log,
        &project.logging,
        env!("CARGO_PKG_NAME"),
    )?;

    match cli.command {
        Command::Run { args } => cmd_run(args, project),
        Command::Assemble { program } => cmd_assemble(&program),
        Command::Info { robot } => cmd_info(&robot),
    }
}

fn cmd_run(args: MatchArgs, project: ProjectConfig) -> Result<()> {
    let match_config = resolve_match_config(&args, &project)?;
    let result = run_match(match_config);

    println!("=== Match Result ===");
    println!("Ticks elapsed: {}", result.ticks_elapsed);
    match result.winner {
        Some(id) => {
            let winner_name = &result.stats[id as usize].name;
            println!("Winner: {} (id {})", winner_name, id);
        }
        None => println!("Winner: Draw (no winner)"),
    }

    println!("\n--- Robot Stats ---");
    for stat in &result.stats {
        println!(
            "  {} (id {}): alive={}, damage_dealt={}, damage_taken={}, shots_fired={}",
            stat.name, stat.id, stat.alive, stat.damage_dealt, stat.damage_taken, stat.shots_fired
        );
    }

    Ok(())
}

fn cmd_assemble(path: &Path) -> Result<()> {
    let source = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read '{}'", path.display()))?;

    match assemble(&source) {
        Ok(program) => {
            println!("OK: {} instructions", program.instructions.len());
        }
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    }

    Ok(())
}

fn cmd_info(path: &Path) -> Result<()> {
    let config = RobotConfig::load(path)?;

    let constants = SimConstants::default();

    println!("=== Robot Info ===");
    println!("Name:    {}", config.name);
    println!("Program: {}", config.program);
    println!();
    println!(
        "Loadout ({}/{} points):",
        config.loadout.total_points(),
        constants.point_budget
    );
    println!("  HP:        {} pts", config.loadout.hp);
    println!("  Speed:     {} pts", config.loadout.speed);
    println!("  Armor:     {} pts", config.loadout.armor);
    println!("  Gun Power: {} pts", config.loadout.gun_power);
    println!();

    let max_hp = constants.base_hp + constants.hp_per_point * config.loadout.hp as i32;
    let max_speed = constants.base_speed + constants.speed_per_point * config.loadout.speed as f32;
    let armor = constants.base_armor + config.loadout.armor as i32;
    let gun_power =
        constants.base_gun_power + constants.gun_power_per_point * config.loadout.gun_power as i32;

    println!("Derived Stats:");
    println!("  Max HP:    {}", max_hp);
    println!("  Max Speed: {:.1}", max_speed);
    println!("  Armor:     {}", armor);
    println!("  Gun Power: {}", gun_power);

    if let Err(e) = config.validate(&constants) {
        println!();
        println!("WARNING: {e}");
    }

    Ok(())
}
