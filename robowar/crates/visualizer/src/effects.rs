use bevy::prelude::*;

use crate::simulation::SimulationState;

#[derive(Debug, Clone)]
pub enum VisualEvent {
    DamageFlash { robot_id: u32 },
    MuzzleFlash { position: (f32, f32), direction: f32 },
    Destruction { position: (f32, f32) },
}

#[derive(Resource, Default)]
pub struct VisualEvents {
    pub events: Vec<VisualEvent>,
}

#[derive(Resource, Default)]
pub struct PrevTickState {
    pub hp: Vec<(u32, i32)>,
    pub alive: Vec<(u32, bool)>,
    pub projectile_count: usize,
}


#[derive(Component)]
pub struct EffectLifetime {
    pub frames_remaining: u32,
    pub max_frames: u32,
}

#[derive(Component)]
pub struct DamageFlashEffect;

#[derive(Component)]
pub struct MuzzleFlashEffect;

#[derive(Component)]
pub struct DestructionEffect;

pub fn capture_pre_tick_state(
    sim_state: Res<SimulationState>,
    mut prev: ResMut<PrevTickState>,
) {
    prev.hp.clear();
    prev.alive.clear();
    for robot in &sim_state.match_state.robots {
        prev.hp.push((robot.id, robot.hp));
        prev.alive.push((robot.id, robot.alive));
    }
    prev.projectile_count = sim_state.match_state.projectiles.len();
}

pub fn detect_events(
    sim_state: Res<SimulationState>,
    prev: Res<PrevTickState>,
    mut events: ResMut<VisualEvents>,
) {
    events.events.clear();

    for robot in &sim_state.match_state.robots {
        if let Some(&(_, prev_hp)) = prev.hp.iter().find(|(id, _)| *id == robot.id)
            && robot.hp < prev_hp
        {
            events.events.push(VisualEvent::DamageFlash { robot_id: robot.id });
        }

        if let Some(&(_, was_alive)) = prev.alive.iter().find(|(id, _)| *id == robot.id)
            && was_alive && !robot.alive
        {
            events.events.push(VisualEvent::Destruction {
                position: robot.position,
            });
        }
    }

    if sim_state.match_state.projectiles.len() > prev.projectile_count {
        let new_count = sim_state.match_state.projectiles.len() - prev.projectile_count;
        for proj in sim_state.match_state.projectiles.iter().rev().take(new_count) {
            events.events.push(VisualEvent::MuzzleFlash {
                position: proj.position,
                direction: proj.direction,
            });
        }
    }
}

pub fn spawn_effects(
    mut commands: Commands,
    events: Res<VisualEvents>,
    sim_state: Res<SimulationState>,
) {
    for event in &events.events {
        match event {
            VisualEvent::DamageFlash { robot_id } => {
                if let Some(robot) = sim_state.match_state.robots.iter().find(|r| r.id == *robot_id) {
                    commands.spawn((
                        DamageFlashEffect,
                        EffectLifetime { frames_remaining: 6, max_frames: 6 },
                        Sprite {
                            color: Color::srgba(1.0, 1.0, 1.0, 0.8),
                            custom_size: Some(Vec2::new(40.0, 40.0)),
                            ..default()
                        },
                        Transform::from_xyz(robot.position.0, robot.position.1, 15.0),
                    ));
                }
            }
            VisualEvent::MuzzleFlash { position, direction } => {
                let offset_x = direction.cos() * 20.0;
                let offset_y = direction.sin() * 20.0;
                commands.spawn((
                    MuzzleFlashEffect,
                    EffectLifetime { frames_remaining: 4, max_frames: 4 },
                    Sprite {
                        color: Color::srgba(1.0, 0.9, 0.3, 1.0),
                        custom_size: Some(Vec2::new(12.0, 12.0)),
                        ..default()
                    },
                    Transform::from_xyz(position.0 + offset_x, position.1 + offset_y, 15.0),
                ));
            }
            VisualEvent::Destruction { position, .. } => {
                commands.spawn((
                    DestructionEffect,
                    EffectLifetime { frames_remaining: 20, max_frames: 20 },
                    Sprite {
                        color: Color::srgba(1.0, 0.5, 0.1, 1.0),
                        custom_size: Some(Vec2::new(20.0, 20.0)),
                        ..default()
                    },
                    Transform::from_xyz(position.0, position.1, 20.0),
                ));
            }
        }
    }
}

pub fn tick_effects(
    mut commands: Commands,
    mut query: Query<(Entity, &mut EffectLifetime, &mut Sprite, &mut Transform, Option<&DestructionEffect>)>,
) {
    for (entity, mut lifetime, mut sprite, mut transform, is_destruction) in &mut query {
        if lifetime.frames_remaining == 0 {
            commands.entity(entity).despawn();
            continue;
        }

        lifetime.frames_remaining -= 1;
        let progress = lifetime.frames_remaining as f32 / lifetime.max_frames as f32;

        let color = sprite.color.to_srgba();
        sprite.color = Color::srgba(color.red, color.green, color.blue, progress);

        if is_destruction.is_some() {
            let scale = 1.0 + (1.0 - progress) * 4.0;
            transform.scale = Vec3::splat(scale);
        }
    }
}
