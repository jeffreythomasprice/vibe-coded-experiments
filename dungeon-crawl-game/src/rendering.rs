use std::collections::HashSet;

use bevy::prelude::*;

use crate::dungeon::Direction;

pub const ROOM_SIZE: f32 = 120.0;
pub const DOOR_WIDTH: f32 = 30.0;
pub const DOOR_DEPTH: f32 = 10.0;
pub const ROOM_GAP: f32 = 8.0;
pub const CELL_SIZE: f32 = ROOM_SIZE + ROOM_GAP;

const ROOM_COLOR: Color = Color::srgb(0.25, 0.22, 0.3);
const DOOR_COLOR: Color = Color::srgb(0.6, 0.45, 0.2);
const GHOST_COLOR: Color = Color::srgba(0.4, 0.8, 0.4, 0.25);

#[derive(Component)]
pub struct RoomSprite(pub IVec2);

#[derive(Component)]
pub struct GhostPlacement {
    pub pos: IVec2,
    pub rotations: Vec<u8>,
}

pub fn grid_to_world(pos: IVec2) -> Vec2 {
    Vec2::new(pos.x as f32 * CELL_SIZE, pos.y as f32 * CELL_SIZE)
}

pub fn spawn_room_visuals(commands: &mut Commands, pos: IVec2, doors: &HashSet<Direction>) {
    let world = grid_to_world(pos);

    commands
        .spawn((
            Sprite {
                color: ROOM_COLOR,
                custom_size: Some(Vec2::splat(ROOM_SIZE)),
                ..default()
            },
            Transform::from_translation(world.extend(0.0)),
            RoomSprite(pos),
        ))
        .with_children(|parent| {
            for dir in doors {
                let (offset, size) = door_transform(*dir);
                parent.spawn((
                    Sprite {
                        color: DOOR_COLOR,
                        custom_size: Some(size),
                        ..default()
                    },
                    Transform::from_translation(offset.extend(1.0)),
                ));
            }
        });
}

fn door_transform(dir: Direction) -> (Vec2, Vec2) {
    let half = ROOM_SIZE / 2.0;
    match dir {
        Direction::North => (Vec2::new(0.0, half), Vec2::new(DOOR_WIDTH, DOOR_DEPTH)),
        Direction::South => (Vec2::new(0.0, -half), Vec2::new(DOOR_WIDTH, DOOR_DEPTH)),
        Direction::East => (Vec2::new(half, 0.0), Vec2::new(DOOR_DEPTH, DOOR_WIDTH)),
        Direction::West => (Vec2::new(-half, 0.0), Vec2::new(DOOR_DEPTH, DOOR_WIDTH)),
    }
}

pub fn spawn_ghost(commands: &mut Commands, pos: IVec2, rotations: Vec<u8>) {
    let world = grid_to_world(pos);
    commands.spawn((
        Sprite {
            color: GHOST_COLOR,
            custom_size: Some(Vec2::splat(ROOM_SIZE)),
            ..default()
        },
        Transform::from_translation(world.extend(-1.0)),
        GhostPlacement { pos, rotations },
    ));
}

pub fn fit_camera(
    rooms: Query<&RoomSprite>,
    mut camera: Query<&mut Transform, (With<Camera2d>, Without<RoomSprite>)>,
) {
    if rooms.is_empty() {
        return;
    }

    let mut min = Vec2::splat(f32::MAX);
    let mut max = Vec2::splat(f32::MIN);

    for room in &rooms {
        let w = grid_to_world(room.0);
        min = min.min(w);
        max = max.max(w);
    }

    let padding = CELL_SIZE * 2.0;
    let center = (min + max) / 2.0;
    let extent = (max - min) / 2.0 + Vec2::splat(padding);

    for mut transform in &mut camera {
        transform.translation.x = center.x;
        transform.translation.y = center.y;
        let scale = (extent.x / 600.0).max(extent.y / 400.0).max(1.0);
        transform.scale = Vec3::splat(scale);
    }
}
