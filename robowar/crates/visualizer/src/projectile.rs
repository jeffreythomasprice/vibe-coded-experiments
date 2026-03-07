use bevy::prelude::*;

use crate::simulation::SimulationState;

#[derive(Component)]
pub struct ProjectileMarker {
    pub id: u32,
}

pub fn sync_projectiles(
    mut commands: Commands,
    sim_state: Res<SimulationState>,
    mut query: Query<(Entity, &ProjectileMarker, &mut Transform)>,
) {
    let sim_projectiles = &sim_state.match_state.projectiles;

    // Update existing or despawn removed projectiles
    for (entity, marker, mut transform) in &mut query {
        if let Some(proj) = sim_projectiles.iter().find(|p| p.id == marker.id) {
            transform.translation.x = proj.position.0;
            transform.translation.y = proj.position.1;
        } else {
            commands.entity(entity).despawn();
        }
    }

    // Spawn new projectiles not yet in ECS
    let existing_ids: Vec<u32> = query.iter().map(|(_, m, _)| m.id).collect();
    for proj in sim_projectiles {
        if !existing_ids.contains(&proj.id) {
            commands.spawn((
                ProjectileMarker { id: proj.id },
                Sprite {
                    color: Color::srgb(1.0, 1.0, 0.2),
                    custom_size: Some(Vec2::new(6.0, 6.0)),
                    ..default()
                },
                Transform::from_xyz(proj.position.0, proj.position.1, 5.0),
            ));
        }
    }
}
