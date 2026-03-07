use anyhow::Result;
use bevy::log::LogPlugin;
use bevy::prelude::*;
use clap::Parser;

use robowar_shared::config::cli_args::{LogArgs, MatchArgs};
use robowar_shared::config::logging::init_logging;
use robowar_shared::config::project::ProjectConfig;
use robowar_shared::config::resolve::resolve_match_config;

mod arena;
mod controls;
mod effects;
mod hud;
mod keybindings;
mod menu;
mod projectile;
mod robot;
mod simulation;

use simulation::SimulationPlugin;

#[derive(Parser)]
#[command(name = "robowar-viz", about = "Robot arena combat visualizer")]
struct Cli {
    #[command(flatten)]
    log_args: LogArgs,

    #[command(flatten)]
    match_args: MatchArgs,

    /// Simulation speed multiplier
    #[arg(long)]
    speed: Option<f32>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let project = ProjectConfig::load(&cli.match_args.config)?;

    init_logging(
        cli.log_args.log_dir.as_deref(),
        cli.log_args.no_file_log,
        &project.logging,
        env!("CARGO_PKG_NAME"),
    )?;

    let speed = cli.speed.or(project.visualizer.speed).unwrap_or(1.0);

    let match_config = resolve_match_config(&cli.match_args, &project)?;

    let keybindings: keybindings::Keybindings = project.visualizer.keybindings.into();

    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "RoboWar".into(),
                        resolution: (1024.0, 768.0).into(),
                        ..default()
                    }),
                    ..default()
                })
                .disable::<LogPlugin>(),
        )
        .insert_resource(keybindings)
        .add_plugins(SimulationPlugin::new(match_config, speed))
        .run();

    Ok(())
}
