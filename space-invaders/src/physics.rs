use std::collections::HashMap;

use glam::Vec2;
use rapier2d::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct PhysicsActorId(pub u64);

pub enum ColliderShape {
    Ball { radius: f32 },
    Cuboid { half_extents: Vec2 },
    ConvexHull { points: Vec<Vec2> },
}

pub enum PhysicsBodyType {
    Dynamic,
    Kinematic,
}

pub struct PhysicsActorDesc {
    pub position: Vec2,
    pub rotation: f32,
    pub velocity: Vec2,
    pub angular_velocity: f32,
    pub shape: ColliderShape,
    pub body_type: PhysicsBodyType,
    pub restitution: f32,
    /// (memberships, filter) bitmasks for collision filtering. None = collide with everything.
    pub interaction_groups: Option<(u32, u32)>,
}

struct PhysicsActor {
    body_handle: RigidBodyHandle,
    collider_handle: ColliderHandle,
}

pub struct PhysicsWorld {
    pipeline: PhysicsPipeline,
    integration_params: IntegrationParameters,
    island_manager: IslandManager,
    broad_phase: DefaultBroadPhase,
    narrow_phase: NarrowPhase,
    rigid_body_set: RigidBodySet,
    collider_set: ColliderSet,
    impulse_joint_set: ImpulseJointSet,
    multibody_joint_set: MultibodyJointSet,
    ccd_solver: CCDSolver,
    actors: HashMap<PhysicsActorId, PhysicsActor>,
    collider_to_id: HashMap<ColliderHandle, PhysicsActorId>,
    next_id: u64,
}

impl PhysicsWorld {
    pub fn new() -> Self {
        Self {
            pipeline: PhysicsPipeline::new(),
            integration_params: IntegrationParameters::default(),
            island_manager: IslandManager::new(),
            broad_phase: DefaultBroadPhase::new(),
            narrow_phase: NarrowPhase::new(),
            rigid_body_set: RigidBodySet::new(),
            collider_set: ColliderSet::new(),
            impulse_joint_set: ImpulseJointSet::new(),
            multibody_joint_set: MultibodyJointSet::new(),
            ccd_solver: CCDSolver::new(),
            actors: HashMap::new(),
            collider_to_id: HashMap::new(),
            next_id: 0,
        }
    }

    pub fn add_actor(&mut self, desc: PhysicsActorDesc) -> PhysicsActorId {
        let id = PhysicsActorId(self.next_id);
        self.next_id += 1;

        let rb = match desc.body_type {
            PhysicsBodyType::Dynamic => RigidBodyBuilder::dynamic()
                .translation(vec2_to_na(desc.position))
                .rotation(desc.rotation)
                .linvel(vec2_to_na(desc.velocity))
                .angvel(desc.angular_velocity)
                .gravity_scale(0.0)
                .build(),
            PhysicsBodyType::Kinematic => RigidBodyBuilder::kinematic_velocity_based()
                .translation(vec2_to_na(desc.position))
                .rotation(desc.rotation)
                .linvel(vec2_to_na(desc.velocity))
                .angvel(desc.angular_velocity)
                .build(),
        };

        let body_handle = self.rigid_body_set.insert(rb);

        let collider_shape: rapier2d::geometry::SharedShape = match desc.shape {
            ColliderShape::Ball { radius } => SharedShape::ball(radius),
            ColliderShape::Cuboid { half_extents } => {
                SharedShape::cuboid(half_extents.x, half_extents.y)
            }
            ColliderShape::ConvexHull { points } => {
                let na_points: Vec<nalgebra::Point2<f32>> =
                    points.iter().map(|p| nalgebra::Point2::new(p.x, p.y)).collect();
                SharedShape::convex_hull(&na_points).unwrap_or_else(|| SharedShape::ball(1.0))
            }
        };

        let mut cb = ColliderBuilder::new(collider_shape).restitution(desc.restitution);
        if let Some((memberships, filter)) = desc.interaction_groups {
            cb = cb.collision_groups(InteractionGroups::new(
                Group::from_bits_truncate(memberships),
                Group::from_bits_truncate(filter),
            ));
        }
        let collider = cb.build();

        let collider_handle =
            self.collider_set
                .insert_with_parent(collider, body_handle, &mut self.rigid_body_set);

        self.collider_to_id.insert(collider_handle, id);
        self.actors.insert(
            id,
            PhysicsActor {
                body_handle,
                collider_handle,
            },
        );

        id
    }

    pub fn remove_actor(&mut self, id: PhysicsActorId) {
        if let Some(actor) = self.actors.remove(&id) {
            self.collider_to_id.remove(&actor.collider_handle);
            self.rigid_body_set.remove(
                actor.body_handle,
                &mut self.island_manager,
                &mut self.collider_set,
                &mut self.impulse_joint_set,
                &mut self.multibody_joint_set,
                true,
            );
        }
    }

    pub fn step(&mut self, dt: f32) {
        self.integration_params.dt = dt;
        self.pipeline.step(
            &vector![0.0, 0.0],
            &self.integration_params,
            &mut self.island_manager,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.rigid_body_set,
            &mut self.collider_set,
            &mut self.impulse_joint_set,
            &mut self.multibody_joint_set,
            &mut self.ccd_solver,
            None,
            &(),
            &(),
        );
    }

    pub fn position(&self, id: PhysicsActorId) -> Option<Vec2> {
        let actor = self.actors.get(&id)?;
        let rb = self.rigid_body_set.get(actor.body_handle)?;
        Some(na_to_vec2(*rb.translation()))
    }

    pub fn rotation(&self, id: PhysicsActorId) -> Option<f32> {
        let actor = self.actors.get(&id)?;
        let rb = self.rigid_body_set.get(actor.body_handle)?;
        Some(rb.rotation().angle())
    }

    pub fn velocity(&self, id: PhysicsActorId) -> Option<Vec2> {
        let actor = self.actors.get(&id)?;
        let rb = self.rigid_body_set.get(actor.body_handle)?;
        Some(na_to_vec2(*rb.linvel()))
    }

    pub fn set_velocity(&mut self, id: PhysicsActorId, vel: Vec2) {
        if let Some(actor) = self.actors.get(&id)
            && let Some(rb) = self.rigid_body_set.get_mut(actor.body_handle)
        {
            rb.set_linvel(vec2_to_na(vel), true);
        }
    }

    pub fn set_angular_velocity(&mut self, id: PhysicsActorId, angvel: f32) {
        if let Some(actor) = self.actors.get(&id)
            && let Some(rb) = self.rigid_body_set.get_mut(actor.body_handle)
        {
            rb.set_angvel(angvel, true);
        }
    }

    pub fn set_position(&mut self, id: PhysicsActorId, pos: Vec2) {
        if let Some(actor) = self.actors.get(&id)
            && let Some(rb) = self.rigid_body_set.get_mut(actor.body_handle)
        {
            rb.set_translation(vec2_to_na(pos), true);
        }
    }

    pub fn active_collision_pairs(&self) -> Vec<(PhysicsActorId, PhysicsActorId)> {
        self.narrow_phase
            .contact_pairs()
            .filter(|pair| pair.has_any_active_contact)
            .filter_map(|pair| {
                let a = self.collider_to_id.get(&pair.collider1)?;
                let b = self.collider_to_id.get(&pair.collider2)?;
                Some((*a, *b))
            })
            .collect()
    }
}

fn vec2_to_na(v: Vec2) -> nalgebra::Vector2<f32> {
    nalgebra::Vector2::new(v.x, v.y)
}

fn na_to_vec2(v: nalgebra::Vector2<f32>) -> Vec2 {
    Vec2::new(v.x, v.y)
}
