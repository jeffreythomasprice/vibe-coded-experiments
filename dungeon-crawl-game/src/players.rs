use std::collections::VecDeque;

use bevy::prelude::*;

use crate::rendering::{self, PlayerDot};
use crate::types::*;

pub fn check_pending_players(
    mut commands: Commands,
    pending: Option<Res<PendingPlayers>>,
) {
    let Some(pending) = pending else { return };
    let mut lock = pending.0.lock().unwrap();
    if let Some(generated) = lock.take() {
        let infos: Vec<PlayerInfo> = generated
            .into_iter()
            .enumerate()
            .map(|(i, player)| PlayerInfo {
                player,
                color: PLAYER_COLORS[i % PLAYER_COLORS.len()],
                location: IVec2::ZERO,
                remaining_move: 0,
                destination: None,
                path: VecDeque::new(),
            })
            .collect();
        commands.insert_resource(Players(infos));
        commands.remove_resource::<PendingPlayers>();
    }
}

pub fn update_player_hud(
    players: Option<Res<Players>>,
    mut names: Query<(&mut Text, &PlayerHudName)>,
) {
    let Some(players) = players else { return };
    if !players.is_changed() {
        return;
    }
    for (mut text, hud_name) in &mut names {
        if let Some(info) = players.0.get(hud_name.0) {
            **text = info.player.name.clone();
        }
    }
}

pub fn update_player_dots(
    mut commands: Commands,
    players: Option<Res<Players>>,
    existing_dots: Query<Entity, With<PlayerDot>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let Some(players) = players else { return };
    if !players.is_changed() {
        return;
    }

    for entity in &existing_dots {
        commands.entity(entity).despawn();
    }

    let dot_size = 14.0_f32;
    let total_width = players.0.len() as f32 * dot_size;
    let start_offset = -total_width / 2.0 + dot_size / 2.0;

    for (i, info) in players.0.iter().enumerate() {
        let world = rendering::grid_to_world(info.location);
        let offset_x = start_offset + i as f32 * dot_size;
        let pos = Vec3::new(world.x + offset_x, world.y - rendering::ROOM_SIZE / 2.0 + dot_size, 3.0);
        commands.spawn((
            Mesh2d(meshes.add(Circle::new(dot_size / 2.0))),
            MeshMaterial2d(materials.add(ColorMaterial::from_color(info.color))),
            Transform::from_translation(pos),
            PlayerDot(i),
        ));
    }
}

pub fn update_selected_player_highlight(
    mut commands: Commands,
    selected: Res<SelectedPlayer>,
    players: Option<Res<Players>>,
    existing: Query<Entity, With<SelectedPlayerHighlight>>,
    dots: Query<(&PlayerDot, &Transform)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    if !selected.is_changed() && !players.as_ref().map_or(false, |p| p.is_changed()) {
        return;
    }

    for entity in &existing {
        commands.entity(entity).despawn();
    }

    let Some(idx) = selected.0 else { return };
    let Some(ref players) = players else { return };
    if players.0.get(idx).is_none() { return }

    for (dot, transform) in &dots {
        if dot.0 == idx {
            let ring_radius = 10.0;
            commands.spawn((
                Mesh2d(meshes.add(bevy::math::primitives::Annulus::new(ring_radius - 2.0, ring_radius))),
                MeshMaterial2d(materials.add(ColorMaterial::from_color(Color::WHITE))),
                Transform::from_translation(transform.translation + Vec3::Z * 0.5),
                SelectedPlayerHighlight,
            ));
            break;
        }
    }
}

pub fn update_selected_hud_highlight(
    selected: Res<SelectedPlayer>,
    mut colors: Query<(&PlayerHudColor, &mut BorderColor)>,
) {
    if !selected.is_changed() { return; }
    for (hud, mut border) in &mut colors {
        *border = if selected.0 == Some(hud.0) {
            BorderColor(Color::WHITE)
        } else {
            BorderColor(Color::NONE)
        };
    }
}
