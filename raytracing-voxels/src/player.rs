use glam::{IVec3, Vec3};

use crate::world::World;

const EPSILON: f32 = 1e-4;

pub const PLAYER_WIDTH: f32 = 0.6;
pub const PLAYER_HEIGHT: f32 = 1.8;
pub const EYE_HEIGHT: f32 = 1.62;
pub const MOVE_SPEED: f32 = 7.5;
pub const GRAVITY: f32 = 28.0;
pub const MAX_FALL_SPEED: f32 = 50.0;
pub const JUMP_VELOCITY: f32 = 11.0;
pub const STEP_HEIGHT: f32 = 1.0;

#[derive(Default)]
pub struct InputState {
    pub forward: bool,
    pub back: bool,
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
}

pub struct Player {
    pub feet_pos: Vec3,
    pub velocity: Vec3,
    pub on_ground: bool,
    pub fly_mode: bool,
}

impl Player {
    pub fn new(eye_pos: Vec3) -> Self {
        Self {
            feet_pos: eye_pos - Vec3::new(0.0, EYE_HEIGHT, 0.0),
            velocity: Vec3::ZERO,
            on_ground: false,
            fly_mode: true,
        }
    }

    pub fn eye_position(&self) -> Vec3 {
        self.feet_pos + Vec3::new(0.0, EYE_HEIGHT, 0.0)
    }

    pub fn toggle_fly_mode(&mut self) {
        self.fly_mode = !self.fly_mode;
        if !self.fly_mode {
            self.velocity = Vec3::ZERO;
        }
    }

    pub fn tick(&mut self, dt: f32, input: &InputState, yaw: f32, world: &World) {
        if self.fly_mode {
            self.tick_fly(dt, input, yaw);
        } else {
            self.tick_physics(dt, input, yaw, world);
        }
    }

    fn tick_physics(&mut self, dt: f32, input: &InputState, yaw: f32, world: &World) {
        let was_on_ground = self.on_ground;
        let (sy, cy) = yaw.sin_cos();
        let forward_dir = Vec3::new(-sy, 0.0, -cy);
        let right_dir = Vec3::new(cy, 0.0, -sy);

        let mut wish_dir = Vec3::ZERO;
        if input.forward {
            wish_dir += forward_dir;
        }
        if input.back {
            wish_dir -= forward_dir;
        }
        if input.right {
            wish_dir += right_dir;
        }
        if input.left {
            wish_dir -= right_dir;
        }

        if wish_dir.length_squared() > 0.0 {
            wish_dir = wish_dir.normalize();
        }

        self.velocity.x = wish_dir.x * MOVE_SPEED;
        self.velocity.z = wish_dir.z * MOVE_SPEED;

        if self.on_ground && input.up {
            self.velocity.y = JUMP_VELOCITY;
        }

        self.velocity.y -= GRAVITY * dt;
        self.velocity.y = self.velocity.y.max(-MAX_FALL_SPEED);

        let new_feet_y = self.feet_pos.y + self.velocity.y * dt;
        let candidate = Vec3::new(self.feet_pos.x, new_feet_y, self.feet_pos.z);

        if Self::aabb_collides(candidate, world) {
            if self.velocity.y < 0.0 {
                self.feet_pos.y = new_feet_y.floor() + 1.0;
                self.velocity.y = 0.0;
                self.on_ground = true;
            } else {
                self.feet_pos.y =
                    (new_feet_y + PLAYER_HEIGHT - EPSILON).ceil() - PLAYER_HEIGHT;
                self.velocity.y = 0.0;
            }
        } else {
            self.feet_pos.y = new_feet_y;
            self.on_ground = false;
        }

        let half_w = PLAYER_WIDTH / 2.0;

        let new_feet_x = self.feet_pos.x + self.velocity.x * dt;
        let candidate_x = Vec3::new(new_feet_x, self.feet_pos.y, self.feet_pos.z);
        if Self::aabb_collides(candidate_x, world) {
            let mut stepped = false;
            if self.on_ground
                && let Some(landed) = Self::try_step_up(self.feet_pos, candidate_x, world)
            {
                self.feet_pos = landed;
                self.on_ground = true;
                stepped = true;
            }
            if !stepped {
                if self.velocity.x > 0.0 {
                    self.feet_pos.x = (new_feet_x + half_w).floor() - half_w;
                } else if self.velocity.x < 0.0 {
                    self.feet_pos.x = (new_feet_x - half_w).ceil() + half_w;
                }
                self.velocity.x = 0.0;
            }
        } else {
            self.feet_pos.x = new_feet_x;
        }

        let new_feet_z = self.feet_pos.z + self.velocity.z * dt;
        let candidate_z = Vec3::new(self.feet_pos.x, self.feet_pos.y, new_feet_z);
        if Self::aabb_collides(candidate_z, world) {
            let mut stepped = false;
            if self.on_ground
                && let Some(landed) = Self::try_step_up(self.feet_pos, candidate_z, world)
            {
                self.feet_pos = landed;
                self.on_ground = true;
                stepped = true;
            }
            if !stepped {
                if self.velocity.z > 0.0 {
                    self.feet_pos.z = (new_feet_z + half_w).floor() - half_w;
                } else if self.velocity.z < 0.0 {
                    self.feet_pos.z = (new_feet_z - half_w).ceil() + half_w;
                }
                self.velocity.z = 0.0;
            }
        } else {
            self.feet_pos.z = new_feet_z;
        }

        if was_on_ground && !self.on_ground && self.velocity.y <= 0.0 {
            for i in 1..=(STEP_HEIGHT as i32) {
                let test_y = self.feet_pos.y - i as f32;
                if Self::aabb_collides(
                    Vec3::new(self.feet_pos.x, test_y, self.feet_pos.z),
                    world,
                ) {
                    self.feet_pos.y = test_y.floor() + 1.0;
                    self.on_ground = true;
                    self.velocity.y = 0.0;
                    break;
                }
            }
        }
    }

    fn try_step_up(
        feet_pos: Vec3,
        new_horizontal_pos: Vec3,
        world: &World,
    ) -> Option<Vec3> {
        let stepped_y = feet_pos.y + STEP_HEIGHT;
        let at_stepped = Vec3::new(feet_pos.x, stepped_y, feet_pos.z);
        if Self::aabb_collides(at_stepped, world) {
            return None;
        }

        let moved_at_stepped = Vec3::new(new_horizontal_pos.x, stepped_y, new_horizontal_pos.z);
        if Self::aabb_collides(moved_at_stepped, world) {
            return None;
        }

        let top_y = stepped_y.floor() as i32;
        let bottom_y = feet_pos.y.floor() as i32;
        let mut land_y = feet_pos.y;

        for iy in (bottom_y..=top_y).rev() {
            let test_y = iy as f32;
            let candidate = Vec3::new(new_horizontal_pos.x, test_y, new_horizontal_pos.z);
            if Self::aabb_collides(candidate, world) {
                land_y = test_y.floor() + 1.0;
                break;
            }
            land_y = test_y;
        }

        Some(Vec3::new(new_horizontal_pos.x, land_y, new_horizontal_pos.z))
    }

    fn aabb_collides(feet_pos: Vec3, world: &World) -> bool {
        let half_w = PLAYER_WIDTH / 2.0;
        let min = feet_pos - Vec3::new(half_w, 0.0, half_w);
        let max = feet_pos + Vec3::new(half_w, PLAYER_HEIGHT, half_w);

        let x_start = min.x.floor() as i32;
        let x_end = (max.x - EPSILON).floor() as i32;
        let y_start = min.y.floor() as i32;
        let y_end = (max.y - EPSILON).floor() as i32;
        let z_start = min.z.floor() as i32;
        let z_end = (max.z - EPSILON).floor() as i32;

        for x in x_start..=x_end {
            for y in y_start..=y_end {
                for z in z_start..=z_end {
                    let voxel_pos = IVec3::new(x, y, z);
                    let (chunk_pos, _) = World::world_to_chunk(voxel_pos);
                    if !world.is_chunk_loaded(&chunk_pos) {
                        return true;
                    }
                    if world.get_voxel(voxel_pos) != 0 {
                        return true;
                    }
                }
            }
        }

        false
    }

    fn tick_fly(&mut self, dt: f32, input: &InputState, yaw: f32) {
        let (sy, cy) = yaw.sin_cos();
        let forward_dir = Vec3::new(-sy, 0.0, -cy);
        let right_dir = Vec3::new(cy, 0.0, -sy);

        let mut wish_dir = Vec3::ZERO;
        if input.forward {
            wish_dir += forward_dir;
        }
        if input.back {
            wish_dir -= forward_dir;
        }
        if input.right {
            wish_dir += right_dir;
        }
        if input.left {
            wish_dir -= right_dir;
        }
        if input.up {
            wish_dir.y += 1.0;
        }
        if input.down {
            wish_dir.y -= 1.0;
        }

        if wish_dir.length_squared() > 0.0 {
            wish_dir = wish_dir.normalize();
        }

        self.feet_pos += wish_dir * MOVE_SPEED * dt;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: Vec3, b: Vec3, eps: f32) -> bool {
        (a - b).abs().max_element() < eps
    }

    #[test]
    fn new_computes_correct_feet_pos() {
        let eye = Vec3::new(10.0, 50.0, 20.0);
        let player = Player::new(eye);
        let expected_feet = Vec3::new(10.0, 50.0 - EYE_HEIGHT, 20.0);
        assert!(approx_eq(player.feet_pos, expected_feet, 1e-6));
    }

    #[test]
    fn eye_position_round_trips() {
        let eye = Vec3::new(5.0, 30.0, 15.0);
        let player = Player::new(eye);
        assert!(approx_eq(player.eye_position(), eye, 1e-6));
    }

    #[test]
    fn fly_mode_tick_moves_position() {
        let mut world = World::new();
        let eye = Vec3::new(0.0, 10.0, 0.0);
        let mut player = Player::new(eye);
        assert!(player.fly_mode);

        let input = InputState {
            forward: true,
            ..InputState::default()
        };

        let dt = 1.0;
        let yaw = 0.0;
        player.tick(dt, &input, yaw, &mut world);

        let expected_feet = Vec3::new(0.0, 10.0 - EYE_HEIGHT, 0.0 - MOVE_SPEED);
        assert!(approx_eq(player.feet_pos, expected_feet, 1e-4));
    }

    #[test]
    fn toggle_fly_mode_zeros_velocity() {
        let mut player = Player::new(Vec3::new(0.0, 10.0, 0.0));
        player.velocity = Vec3::new(1.0, 2.0, 3.0);
        player.toggle_fly_mode();
        assert!(!player.fly_mode);
        assert!(approx_eq(player.velocity, Vec3::ZERO, 1e-6));
    }

    #[test]
    fn toggle_fly_mode_back_preserves_position() {
        let mut player = Player::new(Vec3::new(5.0, 20.0, 10.0));
        let pos_before = player.feet_pos;
        player.toggle_fly_mode();
        player.toggle_fly_mode();
        assert!(player.fly_mode);
        assert!(approx_eq(player.feet_pos, pos_before, 1e-6));
    }

    #[test]
    fn aabb_open_air_no_collision() {
        let mut world = World::new();
        world.insert(IVec3::ZERO, crate::chunk::Chunk::new());
        let feet_pos = Vec3::new(8.0, 8.0, 8.0);
        assert!(!Player::aabb_collides(feet_pos, &world));
    }

    #[test]
    fn aabb_overlapping_solid_voxel() {
        let mut world = World::new();
        let mut chunk = crate::chunk::Chunk::new();
        chunk.set(8, 8, 8, 1);
        world.insert(IVec3::ZERO, chunk);
        let feet_pos = Vec3::new(8.0, 8.0, 8.0);
        assert!(Player::aabb_collides(feet_pos, &world));
    }

    #[test]
    fn aabb_unloaded_chunk_treated_as_solid() {
        let world = World::new();
        let feet_pos = Vec3::new(8.0, 8.0, 8.0);
        assert!(Player::aabb_collides(feet_pos, &world));
    }

    #[test]
    fn aabb_boundary_epsilon_no_collision_below() {
        let mut world = World::new();
        let mut chunk = crate::chunk::Chunk::new();
        chunk.set(8, 4, 8, 1);
        world.insert(IVec3::ZERO, chunk);
        let feet_pos = Vec3::new(8.3, 5.0, 8.3);
        assert!(!Player::aabb_collides(feet_pos, &world));
    }

    fn make_floor_world(floor_y: i32) -> World {
        let mut world = World::new();
        let chunk_y = floor_y.div_euclid(16);
        let local_y = floor_y.rem_euclid(16) as usize;
        for cx in -1..=1 {
            for cz in -1..=1 {
                let mut chunk = crate::chunk::Chunk::new();
                for x in 0..16 {
                    for z in 0..16 {
                        chunk.set(x, local_y, z, 1);
                    }
                }
                world.insert(IVec3::new(cx, chunk_y, cz), chunk);
                if chunk_y + 1 != chunk_y {
                    world.insert(IVec3::new(cx, chunk_y + 1, cz), crate::chunk::Chunk::new());
                }
                if chunk_y - 1 != chunk_y {
                    world.insert(IVec3::new(cx, chunk_y - 1, cz), crate::chunk::Chunk::new());
                }
            }
        }
        world
    }

    #[test]
    fn player_falls_and_lands_on_ground() {
        let world = make_floor_world(0);
        let mut player = Player {
            feet_pos: Vec3::new(8.0, 5.0, 8.0),
            velocity: Vec3::ZERO,
            on_ground: false,
            fly_mode: false,
        };
        let input = InputState::default();
        let dt = 1.0 / 60.0;

        for _ in 0..300 {
            player.tick(dt, &input, 0.0, &world);
            if player.on_ground {
                break;
            }
        }

        assert!(player.on_ground);
        assert!(approx_eq(
            Vec3::new(0.0, player.feet_pos.y, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            1e-3
        ));
        assert!((player.velocity.y).abs() < 1e-6);
    }

    #[test]
    fn player_on_ground_stays_put() {
        let world = make_floor_world(0);
        let mut player = Player {
            feet_pos: Vec3::new(8.0, 1.0, 8.0),
            velocity: Vec3::ZERO,
            on_ground: true,
            fly_mode: false,
        };
        let input = InputState::default();
        let dt = 1.0 / 60.0;

        for _ in 0..10 {
            player.tick(dt, &input, 0.0, &world);
        }

        assert!(player.on_ground);
        assert!((player.feet_pos.y - 1.0).abs() < 1e-3);
        assert!((player.velocity.y).abs() < 1e-6);
    }

    #[test]
    fn player_hitting_ceiling_stops() {
        let world = make_floor_world(5);
        let mut player = Player {
            feet_pos: Vec3::new(8.0, 2.0, 8.0),
            velocity: Vec3::new(0.0, 20.0, 0.0),
            on_ground: false,
            fly_mode: false,
        };
        let input = InputState::default();
        let dt = 1.0 / 60.0;

        let mut hit_ceiling = false;
        for _ in 0..300 {
            let prev_vel_y = player.velocity.y;
            player.tick(dt, &input, 0.0, &world);
            if prev_vel_y > 0.0 && player.velocity.y == 0.0 && !player.on_ground {
                hit_ceiling = true;
                break;
            }
        }

        assert!(hit_ceiling);
        assert!((player.velocity.y).abs() < 1e-6);
        assert!(player.feet_pos.y <= 6.0 - PLAYER_HEIGHT + 0.01);
    }

    #[test]
    fn jump_on_ground_sets_velocity() {
        let world = make_floor_world(0);
        let mut player = Player {
            feet_pos: Vec3::new(8.0, 1.0, 8.0),
            velocity: Vec3::ZERO,
            on_ground: true,
            fly_mode: false,
        };
        let input = InputState {
            up: true,
            ..InputState::default()
        };
        let dt = 1.0 / 60.0;

        player.tick(dt, &input, 0.0, &world);

        assert!(player.velocity.y > 0.0 || player.feet_pos.y > 1.0);
    }

    #[test]
    fn no_air_jump() {
        let world = make_floor_world(0);
        let mut player = Player {
            feet_pos: Vec3::new(8.0, 5.0, 8.0),
            velocity: Vec3::new(0.0, -2.0, 0.0),
            on_ground: false,
            fly_mode: false,
        };
        let input = InputState {
            up: true,
            ..InputState::default()
        };
        let dt = 1.0 / 60.0;

        player.tick(dt, &input, 0.0, &world);

        assert!(player.velocity.y < 0.0);
    }

    #[test]
    fn jump_reaches_approximately_two_voxels() {
        let world = make_floor_world(0);
        let mut player = Player {
            feet_pos: Vec3::new(8.0, 1.0, 8.0),
            velocity: Vec3::ZERO,
            on_ground: true,
            fly_mode: false,
        };
        let jump_input = InputState {
            up: true,
            ..InputState::default()
        };
        let no_input = InputState::default();
        let dt = 1.0 / 60.0;
        let start_y = player.feet_pos.y;

        player.tick(dt, &jump_input, 0.0, &world);

        let mut max_y = player.feet_pos.y;
        for _ in 0..300 {
            player.tick(dt, &no_input, 0.0, &world);
            if player.feet_pos.y > max_y {
                max_y = player.feet_pos.y;
            }
            if player.on_ground && player.feet_pos.y <= start_y + 0.01 {
                break;
            }
        }

        let jump_height = max_y - start_y;
        assert!(
            (jump_height - 2.0).abs() < 0.15,
            "expected ~2.0 voxel jump height, got {jump_height}"
        );
    }

    fn make_floor_with_wall_world(floor_y: i32, wall_voxels: &[(i32, i32, i32)]) -> World {
        let mut world = make_floor_world(floor_y);
        for &(wx, wy, wz) in wall_voxels {
            world.set_voxel(IVec3::new(wx, wy, wz), 1);
        }
        world
    }

    #[test]
    fn wall_slide_x_blocked_z_continues() {
        // Wall spans z=4..12 at x=10, player at x=9.5 moving +X and -Z
        let mut wall_voxels = Vec::new();
        for y in 1..=3 {
            for z in 4..=12 {
                wall_voxels.push((10, y, z));
            }
        }
        let world = make_floor_with_wall_world(0, &wall_voxels);
        let mut player = Player {
            feet_pos: Vec3::new(9.5, 1.0, 8.0),
            velocity: Vec3::ZERO,
            on_ground: true,
            fly_mode: false,
        };
        let input = InputState {
            forward: true,
            right: true,
            ..InputState::default()
        };
        let dt = 1.0 / 60.0;
        let yaw = 0.0;
        let start_pos = player.feet_pos;

        for _ in 0..10 {
            player.tick(dt, &input, yaw, &world);
        }

        assert!(
            (player.feet_pos.x - start_pos.x).abs() < 0.3,
            "X should be blocked by wall, moved {}",
            player.feet_pos.x - start_pos.x
        );
        assert!(
            (player.feet_pos.z - start_pos.z).abs() > 0.1,
            "Z should continue (wall slide), moved {}",
            player.feet_pos.z - start_pos.z
        );
    }

    #[test]
    fn wall_slide_z_blocked_x_continues() {
        // Wall spans x=4..12 at z=6, player at z=6.5 moving +X and -Z
        let mut wall_voxels = Vec::new();
        for y in 1..=3 {
            for x in 4..=12 {
                wall_voxels.push((x, y, 6));
            }
        }
        let world = make_floor_with_wall_world(0, &wall_voxels);
        let mut player = Player {
            feet_pos: Vec3::new(8.0, 1.0, 7.5),
            velocity: Vec3::ZERO,
            on_ground: true,
            fly_mode: false,
        };
        let input = InputState {
            forward: true,
            right: true,
            ..InputState::default()
        };
        let dt = 1.0 / 60.0;
        let yaw = 0.0;
        let start_z_before_wall = player.feet_pos.z;

        for _ in 0..60 {
            player.tick(dt, &input, yaw, &world);
        }

        let half_w = PLAYER_WIDTH / 2.0;
        assert!(
            player.feet_pos.z >= 7.0 - half_w - 0.01,
            "Z should be stopped at wall boundary, got {}",
            player.feet_pos.z
        );
        assert!(
            player.feet_pos.z < start_z_before_wall,
            "Z should have moved toward wall"
        );
        assert!(
            (player.feet_pos.x - 8.0).abs() > 0.5,
            "X should continue (wall slide), moved {}",
            player.feet_pos.x - 8.0
        );
    }

    #[test]
    fn corner_blocks_both_axes() {
        let mut wall_voxels = Vec::new();
        for y in 1..=3 {
            for z in 4..=12 {
                wall_voxels.push((10, y, z));
            }
            for x in 4..=12 {
                wall_voxels.push((x, y, 6));
            }
        }
        let world = make_floor_with_wall_world(0, &wall_voxels);
        let mut player = Player {
            feet_pos: Vec3::new(9.0, 1.0, 7.5),
            velocity: Vec3::ZERO,
            on_ground: true,
            fly_mode: false,
        };
        let input = InputState {
            forward: true,
            right: true,
            ..InputState::default()
        };
        let dt = 1.0 / 60.0;
        let yaw = 0.0;

        for _ in 0..60 {
            player.tick(dt, &input, yaw, &world);
        }

        let half_w = PLAYER_WIDTH / 2.0;
        assert!(
            player.feet_pos.x <= 10.0 - half_w + 0.01,
            "X should be stopped at wall, got {}",
            player.feet_pos.x
        );
        assert!(
            player.feet_pos.z >= 7.0 - half_w - 0.01,
            "Z should be stopped at wall, got {}",
            player.feet_pos.z
        );
    }

    #[test]
    fn walking_parallel_to_wall_no_interference() {
        let mut wall_voxels = Vec::new();
        for y in 1..=3 {
            for z in 4..=12 {
                wall_voxels.push((10, y, z));
            }
        }
        let world = make_floor_with_wall_world(0, &wall_voxels);
        let mut player = Player {
            feet_pos: Vec3::new(8.5, 1.0, 8.5),
            velocity: Vec3::ZERO,
            on_ground: true,
            fly_mode: false,
        };
        let input = InputState {
            forward: true,
            ..InputState::default()
        };
        let dt = 1.0 / 60.0;
        let yaw = 0.0;
        let start_pos = player.feet_pos;

        for _ in 0..10 {
            player.tick(dt, &input, yaw, &world);
        }

        assert!(
            (player.feet_pos.z - start_pos.z).abs() > 0.5,
            "Z should move freely parallel to wall"
        );
        assert!(
            (player.feet_pos.x - start_pos.x).abs() < EPSILON,
            "X should not change when moving parallel"
        );
    }

    #[test]
    fn step_up_one_voxel_step() {
        let mut wall_voxels = Vec::new();
        for x in 10..=14 {
            for z in 4..=12 {
                wall_voxels.push((x, 1, z));
            }
        }
        let world = make_floor_with_wall_world(0, &wall_voxels);
        let mut player = Player {
            feet_pos: Vec3::new(9.0, 1.0, 8.0),
            velocity: Vec3::ZERO,
            on_ground: true,
            fly_mode: false,
        };
        let input = InputState {
            right: true,
            ..InputState::default()
        };
        let dt = 1.0 / 60.0;
        let yaw = 0.0;

        for _ in 0..30 {
            player.tick(dt, &input, yaw, &world);
        }

        assert!(
            (player.feet_pos.y - 2.0).abs() < 0.1,
            "player should be on top of step at y=2.0, got {}",
            player.feet_pos.y
        );
        assert!(
            player.feet_pos.x > 10.0,
            "player should have moved past the step edge, x = {}",
            player.feet_pos.x
        );
    }

    #[test]
    fn no_step_up_two_voxel_wall() {
        let mut wall_voxels = Vec::new();
        for z in 4..=12 {
            wall_voxels.push((10, 1, z));
            wall_voxels.push((10, 2, z));
        }
        let world = make_floor_with_wall_world(0, &wall_voxels);
        let half_w = PLAYER_WIDTH / 2.0;
        let mut player = Player {
            feet_pos: Vec3::new(9.5, 1.0, 8.0),
            velocity: Vec3::ZERO,
            on_ground: true,
            fly_mode: false,
        };
        let input = InputState {
            right: true,
            ..InputState::default()
        };
        let dt = 1.0 / 60.0;
        let yaw = 0.0;

        for _ in 0..30 {
            player.tick(dt, &input, yaw, &world);
        }

        assert!(
            player.feet_pos.x < 10.0 - half_w + 0.01,
            "player should be blocked by wall, x = {}",
            player.feet_pos.x
        );
        assert!(
            (player.feet_pos.y - 1.0).abs() < 0.1,
            "player should stay at floor level, y = {}",
            player.feet_pos.y
        );
    }

    #[test]
    fn no_step_up_while_airborne() {
        let mut wall_voxels = Vec::new();
        for z in 4..=12 {
            wall_voxels.push((10, 1, z));
        }
        let world = make_floor_with_wall_world(0, &wall_voxels);
        let mut player = Player {
            feet_pos: Vec3::new(9.5, 3.0, 8.0),
            velocity: Vec3::new(0.0, -1.0, 0.0),
            on_ground: false,
            fly_mode: false,
        };
        let input = InputState {
            right: true,
            ..InputState::default()
        };
        let dt = 1.0 / 60.0;
        let yaw = 0.0;

        player.tick(dt, &input, yaw, &world);

        assert!(!player.on_ground);
    }

    #[test]
    fn no_step_up_with_low_ceiling() {
        let mut wall_voxels = Vec::new();
        for z in 4..=12 {
            wall_voxels.push((10, 1, z));
        }
        let ceiling_y = 1 + (PLAYER_HEIGHT as i32) + 1;
        for x in 8..=12 {
            for z in 4..=12 {
                wall_voxels.push((x, ceiling_y, z));
            }
        }
        let world = make_floor_with_wall_world(0, &wall_voxels);
        let half_w = PLAYER_WIDTH / 2.0;
        let mut player = Player {
            feet_pos: Vec3::new(9.5, 1.0, 8.0),
            velocity: Vec3::ZERO,
            on_ground: true,
            fly_mode: false,
        };
        let input = InputState {
            right: true,
            ..InputState::default()
        };
        let dt = 1.0 / 60.0;
        let yaw = 0.0;

        for _ in 0..30 {
            player.tick(dt, &input, yaw, &world);
        }

        assert!(
            player.feet_pos.x < 10.0 - half_w + 0.01,
            "player should be blocked, x = {}",
            player.feet_pos.x
        );
        assert!(
            (player.feet_pos.y - 1.0).abs() < 0.1,
            "player should stay at floor level, y = {}",
            player.feet_pos.y
        );
    }

    fn make_ledge_world(floor_y: i32, ledge_drop: i32) -> World {
        let mut world = make_floor_world(floor_y);
        for x in 10..=14 {
            for z in 4..=12 {
                for dy in 1..=ledge_drop {
                    world.set_voxel(IVec3::new(x, floor_y - dy, z), 0);
                }
                world.set_voxel(IVec3::new(x, floor_y, z), 0);
                world.set_voxel(IVec3::new(x, floor_y - ledge_drop, z), 1);
            }
        }
        world
    }

    #[test]
    fn step_down_one_voxel_ledge() {
        let world = make_ledge_world(4, 1);
        let mut player = Player {
            feet_pos: Vec3::new(9.5, 5.0, 8.0),
            velocity: Vec3::ZERO,
            on_ground: true,
            fly_mode: false,
        };
        let input = InputState {
            right: true,
            ..InputState::default()
        };
        let dt = 1.0 / 60.0;
        let yaw = 0.0;

        for _ in 0..30 {
            player.tick(dt, &input, yaw, &world);
        }

        assert!(
            player.feet_pos.x > 10.0,
            "player should have moved past ledge, x = {}",
            player.feet_pos.x
        );
        assert!(
            player.on_ground,
            "player should have snapped down and be on ground"
        );
        assert!(
            (player.feet_pos.y - 4.0).abs() < 0.1,
            "player should have snapped to y=4.0, got {}",
            player.feet_pos.y
        );
    }

    #[test]
    fn step_down_two_voxel_ledge_enters_freefall() {
        let world = make_ledge_world(4, 2);
        let mut player = Player {
            feet_pos: Vec3::new(9.5, 5.0, 8.0),
            velocity: Vec3::ZERO,
            on_ground: true,
            fly_mode: false,
        };
        let input = InputState {
            right: true,
            ..InputState::default()
        };
        let dt = 1.0 / 60.0;
        let yaw = 0.0;

        let mut lost_ground = false;
        for _ in 0..30 {
            player.tick(dt, &input, yaw, &world);
            if player.feet_pos.x > 10.0 && !player.on_ground {
                lost_ground = true;
                break;
            }
        }

        assert!(
            lost_ground,
            "player should enter free-fall over a 2-voxel drop"
        );
    }

    #[test]
    fn step_down_not_during_jump() {
        let world = make_ledge_world(4, 1);
        let mut player = Player {
            feet_pos: Vec3::new(9.5, 5.0, 8.0),
            velocity: Vec3::new(0.0, JUMP_VELOCITY, 0.0),
            on_ground: false,
            fly_mode: false,
        };
        let input = InputState {
            right: true,
            ..InputState::default()
        };
        let dt = 1.0 / 60.0;
        let yaw = 0.0;

        player.tick(dt, &input, yaw, &world);

        assert!(
            !player.on_ground,
            "step-down should not activate during a jump"
        );
        assert!(
            player.feet_pos.y > 5.0,
            "player should be rising from jump, y = {}",
            player.feet_pos.y
        );
    }
}
