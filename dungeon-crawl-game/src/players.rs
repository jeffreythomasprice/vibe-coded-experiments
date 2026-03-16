use std::collections::VecDeque;
use std::hash::{Hash, Hasher};

use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};

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
                active_effects: Vec::new(),
                inventory: Vec::new(),
                dead: false,
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

pub fn update_dead_hud(
    players: Option<Res<Players>>,
    mut query: Query<(&mut Node, &PlayerHudDead)>,
) {
    let Some(players) = players else { return };
    if !players.is_changed() { return; }
    for (mut node, hud) in &mut query {
        node.display = if players.0.get(hud.0).is_some_and(|p| p.dead) {
            Display::Flex
        } else {
            Display::None
        };
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

fn dest_offset_in_room(player_idx: usize, dest: IVec2) -> Vec2 {
    let mut hasher = std::hash::DefaultHasher::new();
    player_idx.hash(&mut hasher);
    dest.hash(&mut hasher);
    let h = hasher.finish();
    let margin = rendering::ROOM_SIZE * 0.3;
    let fx = ((h & 0xFFFF) as f32 / 0xFFFF as f32) * 2.0 - 1.0;
    let fy = (((h >> 16) & 0xFFFF) as f32 / 0xFFFF as f32) * 2.0 - 1.0;
    Vec2::new(fx * margin, fy * margin)
}

pub fn update_destination_arrows(
    mut commands: Commands,
    players: Option<Res<Players>>,
    dots: Query<(&PlayerDot, &Transform)>,
    existing: Query<Entity, With<DestinationArrow>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let Some(ref players) = players else { return };
    if !players.is_changed() { return; }

    for entity in &existing {
        commands.entity(entity).despawn();
    }

    for (i, info) in players.0.iter().enumerate() {
        let Some(dest) = info.destination else { continue };
        if info.dead { continue; }

        let dot_pos = dots.iter()
            .find(|(d, _)| d.0 == i)
            .map(|(_, t)| t.translation.truncate());
        let Some(start) = dot_pos else { continue };

        let dest_world = rendering::grid_to_world(dest) + dest_offset_in_room(i, dest);
        let diff = dest_world - start;
        let length = diff.length();
        if length < 1.0 { continue; }

        let dir = diff / length;
        let angle = dir.y.atan2(dir.x);

        let arrow_color = info.color.with_alpha(0.6);

        // Shaft: thin rectangle from start toward end, shortened to leave room for the head
        let head_len = 8.0;
        let shaft_len = (length - head_len).max(0.0);
        let shaft_center = start + dir * (shaft_len / 2.0);
        let shaft_mesh = meshes.add(Rectangle::new(shaft_len, 3.0));
        let shaft_mat = materials.add(ColorMaterial::from_color(arrow_color));
        commands.spawn((
            Mesh2d(shaft_mesh),
            MeshMaterial2d(shaft_mat),
            Transform::from_translation(shaft_center.extend(2.5))
                .with_rotation(Quat::from_rotation_z(angle)),
            DestinationArrow,
        ));

        // Arrowhead triangle
        let tip = start + dir * length;
        let base_center = tip - dir * head_len;
        let perp = Vec2::new(-dir.y, dir.x);
        let half_width = 6.0;

        let p0 = tip;
        let p1 = base_center + perp * half_width;
        let p2 = base_center - perp * half_width;
        let centroid = (p0 + p1 + p2) / 3.0;

        let mut tri = Mesh::new(PrimitiveTopology::TriangleList, default());
        tri.insert_attribute(
            Mesh::ATTRIBUTE_POSITION,
            vec![
                [p0.x - centroid.x, p0.y - centroid.y, 0.0],
                [p1.x - centroid.x, p1.y - centroid.y, 0.0],
                [p2.x - centroid.x, p2.y - centroid.y, 0.0],
            ],
        );
        tri.insert_indices(Indices::U32(vec![0, 1, 2]));

        let head_mat = materials.add(ColorMaterial::from_color(arrow_color));
        commands.spawn((
            Mesh2d(meshes.add(tri)),
            MeshMaterial2d(head_mat),
            Transform::from_translation(centroid.extend(2.5)),
            DestinationArrow,
        ));
    }
}
