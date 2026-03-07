use std::sync::Mutex;

use bevy::prelude::*;

use robowar_shared::config::constants::SimConstants;
use robowar_shared::sim::match_runner::{MatchConfig, build_match_state};
use robowar_shared::sim::tick::{run_tick, MatchState};

use crate::arena;
use crate::controls;
use crate::effects;
use crate::hud;
use crate::menu;
use crate::projectile;
use crate::robot;

#[derive(Resource)]
pub struct SimulationState {
    pub match_state: MatchState,
    pub robot_names: Vec<String>,
}

#[derive(Resource)]
pub struct SimConfig {
    pub constants: SimConstants,
    pub speed: f32,
    pub paused: bool,
    pub max_ticks: u32,
}

#[derive(Resource)]
pub struct TickTimer {
    pub timer: Timer,
}

#[derive(Resource)]
pub struct MatchSetup {
    pub config: MatchConfig,
    pub speed: f32,
}

pub struct SimulationPlugin {
    config: Mutex<Option<MatchConfig>>,
    speed: f32,
}

impl SimulationPlugin {
    pub fn new(config: MatchConfig, speed: f32) -> Self {
        Self {
            config: Mutex::new(Some(config)),
            speed,
        }
    }
}

impl Plugin for SimulationPlugin {
    fn build(&self, app: &mut App) {
        let config = self
            .config
            .lock()
            .unwrap()
            .take()
            .expect("SimulationPlugin::build called more than once");

        let speed = self.speed;

        app.insert_resource(MatchSetup {
            config,
            speed,
        });

        app.init_resource::<effects::VisualEvents>();
        app.init_resource::<effects::PrevTickState>();
        app.init_resource::<menu::MenuState>();

        app.add_systems(
            Startup,
            (
                setup_simulation,
                arena::spawn_camera,
                arena::spawn_arena,
                robot::spawn_robots,
                hud::setup_hud,
                menu::setup_menu,
            )
                .chain(),
        );
        app.add_systems(
            Update,
            (
                controls::handle_input,
                controls::camera_controls,
                menu::update_menu_visibility,
                menu::handle_menu_interaction,
                effects::capture_pre_tick_state,
                step_simulation,
                effects::detect_events,
                effects::spawn_effects,
                effects::tick_effects,
                check_match_end,
                robot::sync_robots,
                projectile::sync_projectiles,
                hud::update_hud,
            ).chain(),
        );
    }
}

fn setup_simulation(mut commands: Commands, setup: Res<MatchSetup>) {
    let (match_state, robot_names, _initial_hp) = build_match_state(&setup.config);

    let tick_rate = setup.config.constants.tick_rate as f32;
    let period = 1.0 / (tick_rate * setup.speed);

    commands.insert_resource(SimulationState {
        match_state,
        robot_names,
    });

    commands.insert_resource(SimConfig {
        constants: setup.config.constants.clone(),
        speed: setup.speed,
        paused: false,
        max_ticks: setup.config.max_ticks,
    });

    commands.insert_resource(TickTimer {
        timer: Timer::from_seconds(period, TimerMode::Repeating),
    });
}

fn step_simulation(
    time: Res<Time>,
    mut tick_timer: ResMut<TickTimer>,
    mut sim_state: ResMut<SimulationState>,
    sim_config: Res<SimConfig>,
) {
    if sim_config.paused || sim_state.match_state.finished {
        return;
    }

    tick_timer.timer.tick(time.delta());

    let ticks_to_run = tick_timer.timer.times_finished_this_tick();
    for _ in 0..ticks_to_run {
        if sim_state.match_state.finished
            || sim_state.match_state.tick_number >= sim_config.max_ticks
        {
            break;
        }
        run_tick(&mut sim_state.match_state);
    }
}

fn check_match_end(mut sim_state: ResMut<SimulationState>, sim_config: Res<SimConfig>) {
    if sim_state.match_state.finished {
        return;
    }
    if sim_state.match_state.tick_number >= sim_config.max_ticks {
        info!(
            "Match reached tick limit ({})",
            sim_config.max_ticks
        );
        let alive: Vec<_> = sim_state.match_state.robots.iter().filter(|r| r.alive).collect();
        let max_hp = alive.iter().map(|r| r.hp).max().unwrap_or(0);
        let leaders: Vec<_> = alive.iter().filter(|r| r.hp == max_hp).collect();
        if leaders.len() == 1 {
            sim_state.match_state.winner = Some(leaders[0].id);
        }
        sim_state.match_state.finished = true;
    }
}
