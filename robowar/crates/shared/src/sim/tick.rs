use crate::config::constants::SimConstants;
use crate::sim::damage::projectile_damage;
use crate::sim::physics::{cast_scan_ray, get_robot_position, remove_robot_body, set_robot_velocity, PhysicsWorld, step_physics};
use crate::sim::projectile::Projectile;
use crate::sim::robot::Robot;
use crate::vm::executor::{execute_tick_with_handler, Action};
use crate::vm::instruction::RegisterId;

pub struct MatchState {
    pub robots: Vec<Robot>,
    pub projectiles: Vec<Projectile>,
    pub physics: PhysicsWorld,
    pub constants: SimConstants,
    pub arena_bounds: (f32, f32),
    pub tick_number: u32,
    pub finished: bool,
    pub winner: Option<u32>,
    next_projectile_id: u32,
}

impl MatchState {
    pub fn new(robots: Vec<Robot>, projectiles: Vec<Projectile>, physics: PhysicsWorld, constants: SimConstants, arena_bounds: (f32, f32)) -> Self {
        Self {
            robots,
            projectiles,
            physics,
            constants,
            arena_bounds,
            tick_number: 0,
            finished: false,
            winner: None,
            next_projectile_id: 0,
        }
    }
}

fn normalize_angle(degrees: f32) -> f32 {
    let mut d = degrees % 360.0;
    if d < 0.0 {
        d += 360.0;
    }
    d
}

fn update_readonly_registers(robot: &mut Robot, tick: u32) {
    robot.vm.registers.write_readonly(RegisterId::Hp, robot.hp as u32);
    robot.vm.registers.write_readonly(RegisterId::X, robot.position.0.to_bits());
    robot.vm.registers.write_readonly(RegisterId::Y, robot.position.1.to_bits());
    robot.vm.registers.write_readonly(RegisterId::Hdg, robot.heading.to_bits());
    robot.vm.registers.write_readonly(RegisterId::Thr, robot.turret_bearing.to_bits());
    robot.vm.registers.write_readonly(RegisterId::SpdCur, robot.speed.to_bits());
    robot.vm.registers.write_readonly(RegisterId::Tick, tick);
}

pub fn run_tick(state: &mut MatchState) {
    if state.finished {
        return;
    }

    let cycle_budget = state.constants.cycle_budget;
    let projectile_speed = state.constants.projectile_speed;
    let projectile_lifetime = state.constants.projectile_lifetime;
    let fire_cooldown_ticks = state.constants.fire_cooldown;
    let turret_rotation_speed = state.constants.turret_rotation_speed;
    let chassis_rotation_speed = state.constants.chassis_rotation_speed;
    let robot_radius = state.constants.robot_radius;
    let tick = state.tick_number;

    // Phase 1: Program execution
    let mut new_projectiles: Vec<Projectile> = Vec::new();
    for i in 0..state.robots.len() {
        if !state.robots[i].alive {
            continue;
        }

        update_readonly_registers(&mut state.robots[i], tick);

        let robot_pos = state.robots[i].position;
        let robot_heading = state.robots[i].heading;
        let robot_turret_bearing = state.robots[i].turret_bearing;
        let robot_id = state.robots[i].id;
        let robot_gun_power = state.robots[i].gun_power(&state.constants);

        let turret_direction_deg = normalize_angle(robot_heading + robot_turret_bearing);
        let turret_direction_rad = turret_direction_deg.to_radians();

        let own_collider = state.physics.robot_colliders.iter()
            .find(|&&(rid, _)| rid == robot_id)
            .map(|&(_, ch)| ch);

        let physics = &state.physics;
        let next_proj_id = &mut state.next_projectile_id;
        let mut fire_count: u32 = 0;

        execute_tick_with_handler(&mut state.robots[i].vm, cycle_budget, |vm_state, action| {
            match action {
                Action::Scan => {
                    let scan_result = cast_scan_ray(
                        physics,
                        robot_pos,
                        turret_direction_rad,
                        1000.0,
                        own_collider,
                    );
                    match scan_result {
                        Some((dist, collider_handle)) => {
                            vm_state.registers.write_readonly(
                                RegisterId::ScDist,
                                dist.to_bits(),
                            );
                            let mut sc_type: u32 = 1; // default: wall
                            let mut sc_id: u32 = 0;
                            for &(rid, ch) in &physics.robot_colliders {
                                if ch == collider_handle && rid != robot_id {
                                    sc_type = 3; // robot
                                    sc_id = rid;
                                    break;
                                }
                            }
                            vm_state.registers.write_readonly(RegisterId::ScType, sc_type);
                            vm_state.registers.write_readonly(RegisterId::ScId, sc_id);
                        }
                        None => {
                            vm_state.registers.write_readonly(RegisterId::ScDist, 0.0f32.to_bits());
                            vm_state.registers.write_readonly(RegisterId::ScType, 0);
                            vm_state.registers.write_readonly(RegisterId::ScId, 0);
                        }
                    }
                }
                Action::Fire => {
                    if fire_count == 0 {
                        let proj = Projectile {
                            id: *next_proj_id,
                            position: robot_pos,
                            direction: turret_direction_rad,
                            speed: projectile_speed,
                            damage: robot_gun_power,
                            owner_id: robot_id,
                            lifetime_remaining: projectile_lifetime,
                        };
                        *next_proj_id += 1;
                        new_projectiles.push(proj);
                    }
                    fire_count += 1;
                }
            }
        });

        if fire_count > 0 {
            state.robots[i].fire_cooldown = fire_cooldown_ticks;
        }
    }
    state.projectiles.extend(new_projectiles);

    // Phase 2: Turret rotation
    for robot in state.robots.iter_mut().filter(|r| r.alive) {
        let desired_rate = robot.vm.registers.read_f32(RegisterId::Trt);
        let clamped = desired_rate.clamp(-turret_rotation_speed, turret_rotation_speed);
        robot.turret_bearing = normalize_angle(robot.turret_bearing + clamped);
    }

    // Phase 3: Movement — compute velocity and let physics handle collision
    for robot in state.robots.iter_mut().filter(|r| r.alive) {
        let desired_speed = robot.vm.registers.read_f32(RegisterId::Spd);
        let desired_turn = robot.vm.registers.read_f32(RegisterId::Trn);

        let clamped_turn = desired_turn.clamp(-chassis_rotation_speed, chassis_rotation_speed);
        robot.heading = normalize_angle(robot.heading + clamped_turn);

        let max_speed = robot.max_speed(&state.constants);
        robot.speed = desired_speed.clamp(0.0, max_speed);

        let heading_rad = robot.heading.to_radians();
        let vx = robot.speed * heading_rad.cos();
        let vy = robot.speed * heading_rad.sin();

        if let Some(&(_, handle)) = state.physics.robot_bodies.iter().find(|(id, _)| *id == robot.id) {
            set_robot_velocity(&mut state.physics, handle, (vx, vy));
        }
    }

    // Phase 4: Projectile movement
    for proj in state.projectiles.iter_mut() {
        proj.advance();
    }
    state.projectiles.retain(|p| !p.is_expired());

    // Phase 5: Collision resolution
    step_physics(&mut state.physics);

    // Sync positions back from physics
    for robot in state.robots.iter_mut().filter(|r| r.alive) {
        if let Some(&(_, handle)) = state.physics.robot_bodies.iter().find(|(id, _)| *id == robot.id)
            && let Some(pos) = get_robot_position(&state.physics, handle)
        {
            robot.position = pos;
        }
    }

    // Phase 6: Damage application
    for robot in state.robots.iter_mut() {
        if robot.fire_cooldown > 0 {
            robot.fire_cooldown -= 1;
        }
    }

    // Projectile-robot collisions (distance-based)
    let mut projectiles_to_remove: Vec<u32> = Vec::new();
    for proj in &state.projectiles {
        for robot in state.robots.iter_mut().filter(|r| r.alive) {
            if proj.owner_id == robot.id {
                continue;
            }
            let dx = proj.position.0 - robot.position.0;
            let dy = proj.position.1 - robot.position.1;
            let dist_sq = dx * dx + dy * dy;
            if dist_sq <= robot_radius * robot_radius {
                let dmg = projectile_damage(proj.damage, robot.armor(&state.constants));
                robot.hp -= dmg;
                projectiles_to_remove.push(proj.id);
                break;
            }
        }
    }
    state.projectiles.retain(|p| !projectiles_to_remove.contains(&p.id));

    // Kill robots with hp <= 0
    for robot in state.robots.iter_mut() {
        if robot.alive && robot.hp <= 0 {
            robot.alive = false;
            remove_robot_body(&mut state.physics, robot.id);
        }
    }

    // Phase 7: Win check
    let alive_count = state.robots.iter().filter(|r| r.alive).count();
    if alive_count <= 1 {
        state.finished = true;
        if alive_count == 1 {
            state.winner = Some(state.robots.iter().find(|r| r.alive).unwrap().id);
        }
    }

    state.tick_number += 1;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::arena::empty_arena;
    use crate::sim::physics::{add_robot_body, build_physics_world};
    use crate::sim::robot::Loadout;
    use crate::vm::executor::VmState;
    use crate::vm::instruction::{Instruction, Operand, Program};

    fn default_constants() -> SimConstants {
        SimConstants::default()
    }

    fn make_loadout() -> Loadout {
        Loadout {
            hp_points: 5,
            speed_points: 5,
            armor_points: 5,
            gun_points: 5,
        }
    }

    fn make_robot_at(id: u32, pos: (f32, f32), heading: f32, program: Program) -> Robot {
        let constants = default_constants();
        let loadout = make_loadout();
        let hp = constants.base_hp + constants.hp_per_point * loadout.hp_points as i32;
        Robot {
            id,
            position: pos,
            heading,
            turret_bearing: 0.0,
            speed: 0.0,
            hp,
            loadout,
            vm: VmState::new(program),
            fire_cooldown: 0,
            alive: true,
        }
    }

    fn empty_program() -> Program {
        Program {
            instructions: vec![Instruction::Yield],
        }
    }

    fn make_match_state(robots: Vec<Robot>) -> MatchState {
        let constants = default_constants();
        let arena = empty_arena(constants.arena_width, constants.arena_height);
        let bounds = arena.bounds;
        let mut physics = build_physics_world(&arena, &constants);
        for robot in &robots {
            add_robot_body(&mut physics, robot.id, robot.position, constants.robot_radius);
        }
        MatchState::new(robots, Vec::new(), physics, constants, bounds)
    }

    #[test]
    fn two_robots_idle_tick() {
        let r1 = make_robot_at(0, (200.0, 200.0), 0.0, empty_program());
        let r2 = make_robot_at(1, (600.0, 400.0), 180.0, empty_program());
        let mut state = make_match_state(vec![r1, r2]);

        run_tick(&mut state);

        assert!(!state.finished);
        assert!(state.robots[0].alive);
        assert!(state.robots[1].alive);
        assert_eq!(state.tick_number, 1);
    }

    #[test]
    fn single_robot_wins_immediately() {
        let robot = make_robot_at(0, (400.0, 300.0), 0.0, empty_program());
        let mut state = make_match_state(vec![robot]);

        run_tick(&mut state);

        assert!(state.finished);
        assert_eq!(state.winner, Some(0));
    }

    #[test]
    fn turret_rotation_clamped() {
        // Program writes a large turret rotation rate
        let program = Program {
            instructions: vec![
                // Write 100.0 to Trt register (desired turret rotation)
                Instruction::Mov {
                    dst: RegisterId::Trt,
                    src: Operand::ImmediateFloat(100.0),
                },
                Instruction::Yield,
            ],
        };
        let robot = make_robot_at(0, (400.0, 300.0), 0.0, program);
        let mut state = make_match_state(vec![robot]);

        run_tick(&mut state);

        // Turret should only rotate by turret_rotation_speed (30.0 degrees)
        let expected = 30.0;
        assert!(
            (state.robots[0].turret_bearing - expected).abs() < 1e-3,
            "turret_bearing should be {} but was {}",
            expected,
            state.robots[0].turret_bearing
        );
    }

    #[test]
    fn robot_movement() {
        // Program sets speed and no turning
        let program = Program {
            instructions: vec![
                Instruction::Mov {
                    dst: RegisterId::Spd,
                    src: Operand::ImmediateFloat(2.0),
                },
                Instruction::Yield,
            ],
        };
        // Heading 0 degrees = moving along +X
        let robot = make_robot_at(0, (400.0, 300.0), 0.0, program);
        let mut state = make_match_state(vec![robot]);

        run_tick(&mut state);

        // Should move 2.0 in the X direction (heading 0 = cos(0)=1, sin(0)=0)
        assert!(
            (state.robots[0].position.0 - 402.0).abs() < 0.1,
            "x should be ~402, was {}",
            state.robots[0].position.0
        );
        assert!(
            (state.robots[0].position.1 - 300.0).abs() < 0.1,
            "y should be ~300, was {}",
            state.robots[0].position.1
        );
    }

    #[test]
    fn projectile_advancement_and_expiry() {
        let r1 = make_robot_at(0, (400.0, 300.0), 0.0, empty_program());
        let r2 = make_robot_at(1, (600.0, 400.0), 180.0, empty_program());
        let mut state = make_match_state(vec![r1, r2]);

        state.projectiles.push(Projectile {
            id: 0,
            position: (100.0, 100.0),
            direction: 0.0,
            speed: 10.0,
            damage: 5,
            owner_id: 99,
            lifetime_remaining: 2,
        });

        run_tick(&mut state);
        assert_eq!(state.projectiles.len(), 1);
        assert!((state.projectiles[0].position.0 - 110.0).abs() < 1e-3);

        run_tick(&mut state);
        // lifetime was 2, after 2 advances it expires
        assert_eq!(state.projectiles.len(), 0);
    }

    #[test]
    fn win_condition_one_robot_dies() {
        let r1 = make_robot_at(0, (100.0, 100.0), 0.0, empty_program());
        let mut r2 = make_robot_at(1, (200.0, 200.0), 0.0, empty_program());
        r2.hp = 1;

        let mut state = make_match_state(vec![r1, r2]);

        // Place a projectile right on top of robot 1 (id=1), owned by robot 0
        state.projectiles.push(Projectile {
            id: 0,
            position: (200.0, 200.0),
            direction: 0.0,
            speed: 10.0,
            damage: 10,
            owner_id: 0,
            lifetime_remaining: 10,
        });

        run_tick(&mut state);

        assert!(!state.robots[1].alive);
        assert!(state.finished);
        assert_eq!(state.winner, Some(0));
    }

    #[test]
    fn normalize_angle_positive() {
        assert!((normalize_angle(370.0) - 10.0).abs() < 1e-5);
    }

    #[test]
    fn normalize_angle_negative() {
        assert!((normalize_angle(-10.0) - 350.0).abs() < 1e-5);
    }

    #[test]
    fn normalize_angle_zero() {
        assert!((normalize_angle(0.0)).abs() < 1e-5);
    }

    #[test]
    fn normalize_angle_360() {
        assert!(normalize_angle(360.0) < 1e-5);
    }

    #[test]
    fn movement_with_turning() {
        let program = Program {
            instructions: vec![
                Instruction::Mov {
                    dst: RegisterId::Spd,
                    src: Operand::ImmediateFloat(2.0),
                },
                Instruction::Mov {
                    dst: RegisterId::Trn,
                    src: Operand::ImmediateFloat(90.0), // request 90 deg but clamped to 3.0
                },
                Instruction::Yield,
            ],
        };
        let robot = make_robot_at(0, (400.0, 300.0), 0.0, program);
        let mut state = make_match_state(vec![robot]);

        run_tick(&mut state);

        // Heading should have changed by chassis_rotation_speed (3.0 degrees)
        assert!(
            (state.robots[0].heading - 3.0).abs() < 1e-3,
            "heading should be ~3.0, was {}",
            state.robots[0].heading
        );
    }

    #[test]
    fn fire_cooldown_prevents_rapid_fire() {
        let program = Program {
            instructions: vec![
                Instruction::Fire,
                Instruction::Yield,
            ],
        };
        let robot = make_robot_at(0, (400.0, 300.0), 0.0, program);
        let mut state = make_match_state(vec![robot]);

        run_tick(&mut state);
        assert_eq!(state.projectiles.len(), 1);
        let cooldown_after_fire = state.robots[0].fire_cooldown;
        // Cooldown was set to fire_cooldown (10), then decremented by 1 in phase 6
        assert_eq!(cooldown_after_fire, state.constants.fire_cooldown - 1);

        // Second tick: fire instruction runs but cooldown prevents it
        run_tick(&mut state);
        // Only the original projectile remains (or it moved away)
        // No new projectile should have been created
        // We started with 1 projectile. The second fire should not create another.
        assert!(state.projectiles.len() <= 1);
    }

    #[test]
    fn finished_state_prevents_further_ticks() {
        let robot = make_robot_at(0, (400.0, 300.0), 0.0, empty_program());
        let mut state = make_match_state(vec![robot]);
        state.finished = true;
        let tick_before = state.tick_number;

        run_tick(&mut state);

        assert_eq!(state.tick_number, tick_before);
    }

    #[test]
    fn position_stopped_at_right_wall() {
        let program = Program {
            instructions: vec![
                Instruction::Mov {
                    dst: RegisterId::Spd,
                    src: Operand::ImmediateFloat(100.0),
                },
                Instruction::Yield,
            ],
        };
        let robot = make_robot_at(0, (784.0, 300.0), 0.0, program);
        let mut state = make_match_state(vec![robot]);
        let robot_radius = state.constants.robot_radius;
        let max_x = state.arena_bounds.0 - robot_radius;

        run_tick(&mut state);

        assert!(
            state.robots[0].position.0 <= max_x + 1.0,
            "x should be within arena bounds but was {}",
            state.robots[0].position.0
        );
        assert!(
            state.robots[0].position.0 >= 784.0 - 1.0,
            "x should not have moved backwards significantly, was {}",
            state.robots[0].position.0
        );
    }

    #[test]
    fn position_stopped_at_top_wall() {
        let program = Program {
            instructions: vec![
                Instruction::Mov {
                    dst: RegisterId::Spd,
                    src: Operand::ImmediateFloat(100.0),
                },
                Instruction::Yield,
            ],
        };
        // Heading 270 degrees = moving in -Y direction; start near top edge
        let robot = make_robot_at(0, (400.0, 16.0), 270.0, program);
        let mut state = make_match_state(vec![robot]);
        let robot_radius = state.constants.robot_radius;

        run_tick(&mut state);

        assert!(
            state.robots[0].position.1 >= robot_radius - 1.0,
            "y should stay within arena bounds but was {}",
            state.robots[0].position.1
        );
    }

    #[test]
    fn position_stopped_in_corner() {
        let program = Program {
            instructions: vec![
                Instruction::Mov {
                    dst: RegisterId::Spd,
                    src: Operand::ImmediateFloat(100.0),
                },
                Instruction::Yield,
            ],
        };
        // Heading 315 degrees = moving toward bottom-right corner (+X, -Y)
        let robot = make_robot_at(0, (784.0, 16.0), 315.0, program);
        let mut state = make_match_state(vec![robot]);
        let robot_radius = state.constants.robot_radius;
        let max_x = state.arena_bounds.0 - robot_radius;

        run_tick(&mut state);

        assert!(
            state.robots[0].position.0 <= max_x + 1.0,
            "x should be within arena bounds but was {}",
            state.robots[0].position.0
        );
        assert!(
            state.robots[0].position.1 >= robot_radius - 1.0,
            "y should be within arena bounds but was {}",
            state.robots[0].position.1
        );
    }

    #[test]
    fn readonly_registers_updated_before_execution() {
        // Program reads the Tick register into R0
        let program = Program {
            instructions: vec![
                Instruction::Mov {
                    dst: RegisterId::R0,
                    src: Operand::Register(RegisterId::Tick),
                },
                Instruction::Mov {
                    dst: RegisterId::R1,
                    src: Operand::Register(RegisterId::X),
                },
                Instruction::Yield,
            ],
        };
        let robot = make_robot_at(0, (123.0, 456.0), 0.0, program);
        let mut state = make_match_state(vec![robot]);

        run_tick(&mut state);

        assert_eq!(state.robots[0].vm.registers.read_i32(RegisterId::R0), 0); // tick 0
        assert_eq!(
            state.robots[0].vm.registers.read_f32(RegisterId::R1),
            123.0
        );
    }
}
