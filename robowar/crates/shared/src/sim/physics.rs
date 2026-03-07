use rapier2d::prelude::*;

use crate::config::constants::SimConstants;
use crate::sim::arena::{Arena, ArenaBody};

pub struct PhysicsWorld {
    pub rigid_body_set: RigidBodySet,
    pub collider_set: ColliderSet,
    pub query_pipeline: QueryPipeline,
    pub island_manager: IslandManager,
    pub broad_phase: DefaultBroadPhase,
    pub narrow_phase: NarrowPhase,
    pub impulse_joint_set: ImpulseJointSet,
    pub multibody_joint_set: MultibodyJointSet,
    pub ccd_solver: CCDSolver,
    pub physics_pipeline: PhysicsPipeline,
    pub robot_bodies: Vec<(u32, RigidBodyHandle)>,
    pub robot_colliders: Vec<(u32, ColliderHandle)>,
}

pub fn build_physics_world(arena: &Arena, _constants: &SimConstants) -> PhysicsWorld {
    let mut rigid_body_set = RigidBodySet::new();
    let mut collider_set = ColliderSet::new();

    for body in &arena.bodies {
        match body {
            ArenaBody::Rectangle {
                x,
                y,
                width,
                height,
                rotation,
            } => {
                let rb = RigidBodyBuilder::fixed()
                    .position(Isometry::new(vector![*x, *y], *rotation))
                    .build();
                let handle = rigid_body_set.insert(rb);
                let collider = ColliderBuilder::cuboid(*width / 2.0, *height / 2.0).build();
                collider_set.insert_with_parent(collider, handle, &mut rigid_body_set);
            }
            ArenaBody::Circle { x, y, radius } => {
                let rb = RigidBodyBuilder::fixed()
                    .position(Isometry::new(vector![*x, *y], 0.0))
                    .build();
                let handle = rigid_body_set.insert(rb);
                let collider = ColliderBuilder::ball(*radius).build();
                collider_set.insert_with_parent(collider, handle, &mut rigid_body_set);
            }
        }
    }

    let mut query_pipeline = QueryPipeline::new();
    query_pipeline.update(&collider_set);

    PhysicsWorld {
        rigid_body_set,
        collider_set,
        query_pipeline,
        island_manager: IslandManager::new(),
        broad_phase: DefaultBroadPhase::new(),
        narrow_phase: NarrowPhase::new(),
        impulse_joint_set: ImpulseJointSet::new(),
        multibody_joint_set: MultibodyJointSet::new(),
        ccd_solver: CCDSolver::new(),
        physics_pipeline: PhysicsPipeline::new(),
        robot_bodies: Vec::new(),
        robot_colliders: Vec::new(),
    }
}

pub fn add_robot_body(
    world: &mut PhysicsWorld,
    robot_id: u32,
    position: (f32, f32),
    radius: f32,
) -> RigidBodyHandle {
    let rb = RigidBodyBuilder::dynamic()
        .position(Isometry::new(vector![position.0, position.1], 0.0))
        .locked_axes(LockedAxes::ROTATION_LOCKED)
        .linear_damping(0.0)
        .build();
    let handle = world.rigid_body_set.insert(rb);
    let collider = ColliderBuilder::ball(radius)
        .restitution(0.0)
        .friction(0.0)
        .build();
    let collider_handle =
        world
            .collider_set
            .insert_with_parent(collider, handle, &mut world.rigid_body_set);
    world.robot_bodies.push((robot_id, handle));
    world.robot_colliders.push((robot_id, collider_handle));
    handle
}

pub fn remove_robot_body(world: &mut PhysicsWorld, robot_id: u32) {
    if let Some(pos) = world.robot_bodies.iter().position(|(id, _)| *id == robot_id) {
        let (_, body_handle) = world.robot_bodies.remove(pos);
        world.rigid_body_set.remove(
            body_handle,
            &mut world.island_manager,
            &mut world.collider_set,
            &mut world.impulse_joint_set,
            &mut world.multibody_joint_set,
            true,
        );
    }
    world.robot_colliders.retain(|(id, _)| *id != robot_id);
}

pub fn set_robot_velocity(
    world: &mut PhysicsWorld,
    handle: RigidBodyHandle,
    velocity_per_tick: (f32, f32),
) {
    if let Some(rb) = world.rigid_body_set.get_mut(handle) {
        let scale = 1.0 / PHYSICS_DT;
        rb.set_linvel(vector![velocity_per_tick.0 * scale, velocity_per_tick.1 * scale], true);
    }
}

pub fn get_robot_position(world: &PhysicsWorld, handle: RigidBodyHandle) -> Option<(f32, f32)> {
    world
        .rigid_body_set
        .get(handle)
        .map(|rb| {
            let pos = rb.position().translation;
            (pos.x, pos.y)
        })
}

pub fn cast_scan_ray(
    world: &PhysicsWorld,
    origin: (f32, f32),
    direction: f32,
    max_distance: f32,
    exclude_collider: Option<ColliderHandle>,
) -> Option<(f32, ColliderHandle)> {
    let ray = Ray::new(
        point![origin.0, origin.1],
        vector![direction.cos(), direction.sin()],
    );
    let filter = match exclude_collider {
        Some(ch) => QueryFilter::default().exclude_collider(ch),
        None => QueryFilter::default(),
    };
    world
        .query_pipeline
        .cast_ray(
            &world.rigid_body_set,
            &world.collider_set,
            &ray,
            max_distance,
            true,
            filter,
        )
        .map(|(handle, toi)| (toi, handle))
}

pub const PHYSICS_DT: f32 = 1.0 / 60.0;

pub fn step_physics(world: &mut PhysicsWorld) {
    let gravity = vector![0.0, 0.0];
    let integration_parameters = IntegrationParameters {
        length_unit: 100.0,
        normalized_prediction_distance: 0.1,
        contact_natural_frequency: 120.0,
        ..Default::default()
    };

    world.physics_pipeline.step(
        &gravity,
        &integration_parameters,
        &mut world.island_manager,
        &mut world.broad_phase,
        &mut world.narrow_phase,
        &mut world.rigid_body_set,
        &mut world.collider_set,
        &mut world.impulse_joint_set,
        &mut world.multibody_joint_set,
        &mut world.ccd_solver,
        Some(&mut world.query_pipeline),
        &(),
        &(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::arena::empty_arena;

    #[test]
    fn build_empty_arena_physics_world() {
        let arena = empty_arena(800.0, 600.0);
        let constants = SimConstants::default();
        let world = build_physics_world(&arena, &constants);

        assert_eq!(world.rigid_body_set.len(), 4);
        assert_eq!(world.collider_set.len(), 4);
        assert!(world.robot_bodies.is_empty());
        assert!(world.robot_colliders.is_empty());
    }

    #[test]
    fn add_robot_and_move_with_velocity() {
        let arena = empty_arena(800.0, 600.0);
        let constants = SimConstants::default();
        let mut world = build_physics_world(&arena, &constants);

        let handle = add_robot_body(&mut world, 1, (100.0, 100.0), constants.robot_radius);

        let rb = world.rigid_body_set.get(handle).unwrap();
        let pos = rb.position().translation;
        assert!((pos.x - 100.0).abs() < 1e-5);
        assert!((pos.y - 100.0).abs() < 1e-5);

        set_robot_velocity(&mut world, handle, (5.0, 0.0));
        step_physics(&mut world);

        let new_pos = get_robot_position(&world, handle).unwrap();
        assert!((new_pos.0 - 105.0).abs() < 0.5, "x should be ~105, was {}", new_pos.0);
        assert!((new_pos.1 - 100.0).abs() < 0.5, "y should be ~100, was {}", new_pos.1);
    }

    #[test]
    fn ray_cast_hits_wall() {
        let arena = empty_arena(800.0, 600.0);
        let constants = SimConstants::default();
        let world = build_physics_world(&arena, &constants);

        // Cast a ray from center going right -- should hit the right wall
        let result = cast_scan_ray(&world, (400.0, 300.0), 0.0, 1000.0, None);
        assert!(result.is_some());
        let (distance, _handle) = result.unwrap();
        // Right wall center is at x = 800 + 5, with half-extent 5, so inner edge at ~800.
        // From x=400, distance should be roughly 400.
        assert!(distance > 350.0 && distance < 450.0);
    }

    #[test]
    fn ray_cast_misses_when_no_obstacle() {
        let constants = SimConstants::default();
        let arena = Arena {
            bounds: (800.0, 600.0),
            bodies: vec![],
            spawn_points: vec![],
        };
        let world = build_physics_world(&arena, &constants);

        let result = cast_scan_ray(&world, (400.0, 300.0), 0.0, 100.0, None);
        assert!(result.is_none());
    }

    #[test]
    fn robot_body_tracked_in_vectors() {
        let arena = empty_arena(800.0, 600.0);
        let constants = SimConstants::default();
        let mut world = build_physics_world(&arena, &constants);

        add_robot_body(&mut world, 42, (100.0, 100.0), constants.robot_radius);
        add_robot_body(&mut world, 99, (700.0, 500.0), constants.robot_radius);

        assert_eq!(world.robot_bodies.len(), 2);
        assert_eq!(world.robot_colliders.len(), 2);
        assert_eq!(world.robot_bodies[0].0, 42);
        assert_eq!(world.robot_bodies[1].0, 99);
    }

    #[test]
    fn step_physics_does_not_panic() {
        let arena = empty_arena(800.0, 600.0);
        let constants = SimConstants::default();
        let mut world = build_physics_world(&arena, &constants);
        add_robot_body(&mut world, 1, (100.0, 100.0), constants.robot_radius);
        step_physics(&mut world);
        step_physics(&mut world);
    }

    #[test]
    fn robot_stopped_by_wall() {
        let arena = empty_arena(800.0, 600.0);
        let constants = SimConstants::default();
        let mut world = build_physics_world(&arena, &constants);

        let handle = add_robot_body(&mut world, 1, (782.0, 300.0), constants.robot_radius);

        for _ in 0..5 {
            set_robot_velocity(&mut world, handle, (4.5, 0.0));
            step_physics(&mut world);
        }

        let final_pos = get_robot_position(&world, handle).unwrap();
        assert!(final_pos.0 <= 786.0, "Robot should be stopped by wall, but x={:.3}", final_pos.0);
    }
}
