use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand::rngs::StdRng;

use crate::config::constants::SimConstants;
use crate::sim::arena::Arena;
use crate::sim::physics::{add_robot_body, build_physics_world};
use crate::sim::robot::{Loadout, Robot};
use crate::sim::tick::{run_tick, MatchState};
use crate::vm::executor::VmState;
use crate::vm::instruction::Program;

#[derive(Debug, Clone)]
pub enum MatchFormat {
    Duel,
    FreeForAll,
    Tournament,
}

pub struct MatchConfig {
    pub format: MatchFormat,
    pub max_ticks: u32,
    pub arena: Arena,
    pub robot_configs: Vec<RobotEntry>,
    pub constants: SimConstants,
    pub seed: u64,
}

pub struct RobotEntry {
    pub name: String,
    pub program: Program,
    pub loadout: Loadout,
}

#[derive(Debug)]
pub struct MatchResult {
    pub winner: Option<u32>,
    pub ticks_elapsed: u32,
    pub stats: Vec<RobotStats>,
}

#[derive(Debug)]
pub struct RobotStats {
    pub id: u32,
    pub name: String,
    pub damage_dealt: i32,
    pub damage_taken: i32,
    pub shots_fired: u32,
    pub alive: bool,
}

pub fn build_match_state(config: &MatchConfig) -> (MatchState, Vec<String>, Vec<i32>) {
    let mut physics = build_physics_world(&config.arena, &config.constants);

    let mut spawn_points = config.arena.spawn_points.clone();
    let mut rng = StdRng::seed_from_u64(config.seed);
    spawn_points.shuffle(&mut rng);

    let mut robots: Vec<Robot> = Vec::new();
    let mut names: Vec<String> = Vec::new();
    let mut initial_hp: Vec<i32> = Vec::new();

    for (i, entry) in config.robot_configs.iter().enumerate() {
        let spawn = &spawn_points[i % spawn_points.len()];
        let hp = config.constants.base_hp
            + config.constants.hp_per_point * entry.loadout.hp_points as i32;

        let robot = Robot {
            id: i as u32,
            position: spawn.position,
            heading: spawn.facing,
            turret_bearing: 0.0,
            speed: 0.0,
            hp,
            loadout: entry.loadout.clone(),
            vm: VmState::new(entry.program.clone()),
            fire_cooldown: 0,
            alive: true,
        };

        add_robot_body(
            &mut physics,
            robot.id,
            robot.position,
            config.constants.robot_radius,
        );

        names.push(entry.name.clone());
        initial_hp.push(hp);
        robots.push(robot);
    }

    physics.query_pipeline.update(&physics.collider_set);
    let state = MatchState::new(robots, Vec::new(), physics, config.constants.clone(), config.arena.bounds);
    (state, names, initial_hp)
}

pub fn run_match(config: MatchConfig) -> MatchResult {
    let (mut state, _names, initial_hp) = build_match_state(&config);

    while !state.finished && state.tick_number < config.max_ticks {
        run_tick(&mut state);
    }

    if !state.finished {
        let alive: Vec<_> = state.robots.iter().filter(|r| r.alive).collect();
        let max_hp = alive.iter().map(|r| r.hp).max().unwrap_or(0);
        let leaders: Vec<_> = alive.iter().filter(|r| r.hp == max_hp).collect();
        if leaders.len() == 1 {
            state.winner = Some(leaders[0].id);
        }
    }

    let stats: Vec<RobotStats> = state
        .robots
        .iter()
        .enumerate()
        .map(|(i, robot)| {
            let damage_taken = initial_hp[i] - robot.hp;
            RobotStats {
                id: robot.id,
                name: config.robot_configs[i].name.clone(),
                damage_dealt: 0,
                damage_taken,
                shots_fired: 0,
                alive: robot.alive,
            }
        })
        .collect();

    MatchResult {
        winner: state.winner,
        ticks_elapsed: state.tick_number,
        stats,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::arena::empty_arena;
    use crate::vm::instruction::{Instruction, Operand, RegisterId};

    fn default_constants() -> SimConstants {
        SimConstants::default()
    }

    fn balanced_loadout() -> Loadout {
        Loadout {
            hp_points: 5,
            speed_points: 5,
            armor_points: 5,
            gun_points: 5,
        }
    }

    fn idle_program() -> Program {
        Program {
            instructions: vec![Instruction::Yield, Instruction::Jmp { addr: 0 }],
        }
    }

    fn make_config(entries: Vec<RobotEntry>) -> MatchConfig {
        let constants = default_constants();
        let arena = empty_arena(constants.arena_width, constants.arena_height);
        MatchConfig {
            format: MatchFormat::Duel,
            max_ticks: 100,
            arena,
            robot_configs: entries,
            constants,
            seed: 42,
        }
    }

    #[test]
    fn two_idle_robots_hit_tick_limit() {
        let entries = vec![
            RobotEntry {
                name: "IdleBot1".to_string(),
                program: idle_program(),
                loadout: balanced_loadout(),
            },
            RobotEntry {
                name: "IdleBot2".to_string(),
                program: idle_program(),
                loadout: balanced_loadout(),
            },
        ];

        let result = run_match(make_config(entries));

        assert_eq!(result.ticks_elapsed, 100);
        assert!(result.winner.is_none(), "equal-HP robots should draw");
        assert_eq!(result.stats.len(), 2);
        assert!(result.stats[0].alive);
        assert!(result.stats[1].alive);
    }

    #[test]
    fn firing_robot_does_not_crash() {
        let fire_program = Program {
            instructions: vec![
                Instruction::Mov {
                    dst: RegisterId::Trt,
                    src: Operand::ImmediateFloat(5.0),
                },
                Instruction::Fire,
                Instruction::Yield,
                Instruction::Jmp { addr: 0 },
            ],
        };

        let entries = vec![
            RobotEntry {
                name: "Shooter".to_string(),
                program: fire_program,
                loadout: balanced_loadout(),
            },
            RobotEntry {
                name: "IdleBot".to_string(),
                program: idle_program(),
                loadout: balanced_loadout(),
            },
        ];

        let result = run_match(make_config(entries));

        assert_eq!(result.ticks_elapsed, 100);
        assert_eq!(result.stats.len(), 2);
        assert_eq!(result.stats[0].name, "Shooter");
        assert_eq!(result.stats[1].name, "IdleBot");
    }
}
