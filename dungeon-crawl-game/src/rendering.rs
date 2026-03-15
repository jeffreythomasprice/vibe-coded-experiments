use std::collections::HashSet;

use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use bevy::sprite::Anchor;
use bevy::text::TextBounds;

use crate::PlaceholderRoom;
use crate::dungeon::Direction;

pub const ROOM_SIZE: f32 = 120.0;
pub const DOOR_WIDTH: f32 = 30.0;
pub const DOOR_DEPTH: f32 = 10.0;
pub const ROOM_GAP: f32 = 8.0;
pub const CELL_SIZE: f32 = ROOM_SIZE + ROOM_GAP;

const PAN_SPEED: f32 = 500.0;
const ZOOM_SPEED_KEY: f32 = 2.0;
const ZOOM_SPEED_WHEEL: f32 = 0.15;

#[derive(Resource)]
pub struct CameraState {
    pub zoom: f32,
    pub offset: Vec2,
}

const ROOM_COLOR: Color = Color::srgb(0.25, 0.22, 0.3);
const DOOR_COLOR: Color = Color::srgb(0.6, 0.45, 0.2);
const CANDIDATE_COLOR: Color = Color::srgba(0.4, 0.8, 0.4, 0.15);
const VALID_ROOM_COLOR: Color = Color::srgba(0.4, 0.8, 0.4, 0.3);
const VALID_DOOR_COLOR: Color = Color::srgba(0.5, 0.9, 0.5, 0.5);
const INVALID_ROOM_COLOR: Color = Color::srgba(0.8, 0.3, 0.3, 0.3);
const INVALID_DOOR_COLOR: Color = Color::srgba(0.9, 0.4, 0.4, 0.5);

#[derive(Component)]
pub struct RoomSprite(pub IVec2);

#[derive(Component)]
pub struct CandidateMarker(pub IVec2);

#[derive(Component)]
pub struct ActiveGhost;

#[derive(Component)]
pub struct PlayerDot(pub usize);

pub fn grid_to_world(pos: IVec2) -> Vec2 {
    Vec2::new(pos.x as f32 * CELL_SIZE, pos.y as f32 * CELL_SIZE)
}

pub fn spawn_room_visuals(commands: &mut Commands, pos: IVec2, doors: &HashSet<Direction>, name: &str) {
    let world = grid_to_world(pos);
    let font_size = (ROOM_SIZE * 1.4 / name.len() as f32).clamp(8.0, 16.0);
    let padding = 10.0;

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

            parent.spawn((
                Text2d::new(name),
                TextFont {
                    font_size,
                    ..default()
                },
                TextColor(Color::WHITE),
                TextLayout {
                    justify: JustifyText::Center,
                    ..default()
                },
                TextBounds {
                    width: Some(ROOM_SIZE - padding * 2.0),
                    height: Some(ROOM_SIZE - padding * 2.0),
                },
                Anchor::Center,
                Transform::from_translation(Vec3::new(0.0, 0.0, 2.0)),
            ));
        });
}

pub fn spawn_placeholder_room(commands: &mut Commands, pos: IVec2) {
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
            PlaceholderRoom,
        ))
        .with_children(|parent| {
            parent.spawn((
                Text2d::new("?"),
                TextFont { font_size: 16.0, ..default() },
                TextColor(Color::WHITE),
                TextLayout { justify: JustifyText::Center, ..default() },
                Anchor::Center,
                Transform::from_translation(Vec3::new(0.0, 0.0, 2.0)),
            ));
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

pub fn spawn_candidate_marker(commands: &mut Commands, pos: IVec2) {
    let world = grid_to_world(pos);
    commands.spawn((
        Sprite {
            color: CANDIDATE_COLOR,
            custom_size: Some(Vec2::splat(ROOM_SIZE * 0.5)),
            ..default()
        },
        Transform::from_translation(world.extend(-1.0)),
        CandidateMarker(pos),
    ));
}

pub fn spawn_active_ghost(
    commands: &mut Commands,
    pos: IVec2,
    doors: &HashSet<Direction>,
    name: &str,
    valid: bool,
) {
    let world = grid_to_world(pos);
    let (room_color, door_color) = if valid {
        (VALID_ROOM_COLOR, VALID_DOOR_COLOR)
    } else {
        (INVALID_ROOM_COLOR, INVALID_DOOR_COLOR)
    };
    let font_size = (ROOM_SIZE * 1.4 / name.len() as f32).clamp(8.0, 16.0);
    let padding = 10.0;

    commands
        .spawn((
            Sprite {
                color: room_color,
                custom_size: Some(Vec2::splat(ROOM_SIZE)),
                ..default()
            },
            Transform::from_translation(world.extend(-0.5)),
            ActiveGhost,
        ))
        .with_children(|parent| {
            for dir in doors {
                let (offset, size) = door_transform(*dir);
                parent.spawn((
                    Sprite {
                        color: door_color,
                        custom_size: Some(size),
                        ..default()
                    },
                    Transform::from_translation(offset.extend(1.0)),
                ));
            }

            parent.spawn((
                Text2d::new(name),
                TextFont {
                    font_size,
                    ..default()
                },
                TextColor(Color::srgba(1.0, 1.0, 1.0, 0.6)),
                TextLayout {
                    justify: JustifyText::Center,
                    ..default()
                },
                TextBounds {
                    width: Some(ROOM_SIZE - padding * 2.0),
                    height: Some(ROOM_SIZE - padding * 2.0),
                },
                Anchor::Center,
                Transform::from_translation(Vec3::new(0.0, 0.0, 2.0)),
            ));
        });
}

pub fn compute_dungeon_bounds(rooms: &Query<&RoomSprite>) -> Option<(Vec2, Vec2)> {
    if rooms.is_empty() {
        return None;
    }
    let mut min = Vec2::splat(f32::MAX);
    let mut max = Vec2::splat(f32::MIN);
    for room in rooms.iter() {
        let w = grid_to_world(room.0);
        min = min.min(w);
        max = max.max(w);
    }
    let padding = CELL_SIZE * 2.0;
    let center = (min + max) / 2.0;
    let extent = (max - min) / 2.0 + Vec2::splat(padding);
    Some((center, extent))
}

pub fn fit_all_zoom(extent: Vec2) -> f32 {
    (extent.x / 600.0).max(extent.y / 400.0).max(1.0)
}

fn zoom_in_limit() -> f32 {
    CELL_SIZE / (600.0 * 0.8)
}

pub fn apply_camera(
    rooms: Query<&RoomSprite>,
    state: Res<CameraState>,
    mut camera: Query<&mut Transform, (With<Camera2d>, Without<RoomSprite>)>,
) {
    let Some((center, _extent)) = compute_dungeon_bounds(&rooms) else {
        return;
    };
    for mut transform in &mut camera {
        transform.translation.x = center.x + state.offset.x;
        transform.translation.y = center.y + state.offset.y;
        transform.scale = Vec3::splat(state.zoom);
    }
}

pub fn camera_keyboard_pan(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    rooms: Query<&RoomSprite>,
    mut state: ResMut<CameraState>,
) {
    let mut delta = Vec2::ZERO;
    if keys.pressed(KeyCode::ArrowLeft) || keys.pressed(KeyCode::KeyA) {
        delta.x -= 1.0;
    }
    if keys.pressed(KeyCode::ArrowRight) || keys.pressed(KeyCode::KeyD) {
        delta.x += 1.0;
    }
    if keys.pressed(KeyCode::ArrowUp) || keys.pressed(KeyCode::KeyW) {
        delta.y += 1.0;
    }
    if keys.pressed(KeyCode::ArrowDown) || keys.pressed(KeyCode::KeyS) {
        delta.y -= 1.0;
    }
    if delta == Vec2::ZERO {
        return;
    }
    let zoom = state.zoom;
    state.offset += delta.normalize() * PAN_SPEED * zoom * time.delta_secs();
    clamp_pan(&rooms, &mut state);
}

pub fn camera_zoom(
    keys: Res<ButtonInput<KeyCode>>,
    mut wheel: EventReader<MouseWheel>,
    time: Res<Time>,
    rooms: Query<&RoomSprite>,
    mut state: ResMut<CameraState>,
) {
    let mut zoom_delta: f32 = 0.0;
    if keys.pressed(KeyCode::Minus) {
        zoom_delta += ZOOM_SPEED_KEY * time.delta_secs();
    }
    if keys.pressed(KeyCode::Equal) {
        zoom_delta -= ZOOM_SPEED_KEY * time.delta_secs();
    }
    for ev in wheel.read() {
        zoom_delta -= ev.y * ZOOM_SPEED_WHEEL;
    }
    if zoom_delta == 0.0 {
        return;
    }

    let (min_zoom, max_zoom) = zoom_limits(&rooms);
    state.zoom = (state.zoom + zoom_delta * state.zoom).clamp(min_zoom, max_zoom);
    clamp_pan(&rooms, &mut state);
}

#[derive(Resource, Default)]
pub struct DragState {
    pub active: bool,
    pub last_pos: Option<Vec2>,
}

pub fn camera_drag_pan(
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    rooms: Query<&RoomSprite>,
    mut state: ResMut<CameraState>,
    mut drag: ResMut<DragState>,
) {
    if mouse.just_pressed(MouseButton::Right) {
        drag.active = true;
        drag.last_pos = None;
    }
    if mouse.just_released(MouseButton::Right) {
        drag.active = false;
        drag.last_pos = None;
    }
    if !drag.active {
        return;
    }
    let Ok(window) = windows.single() else { return };
    let Some(cursor) = window.cursor_position() else { return };

    if let Some(last) = drag.last_pos {
        let delta = cursor - last;
        // screen-space delta to world-space (y is inverted, scale by zoom)
        state.offset.x -= delta.x * state.zoom;
        state.offset.y += delta.y * state.zoom;
        clamp_pan(&rooms, &mut state);
    }
    drag.last_pos = Some(cursor);
}

fn zoom_limits(rooms: &Query<&RoomSprite>) -> (f32, f32) {
    let min_zoom = zoom_in_limit();
    let max_zoom = match compute_dungeon_bounds(rooms) {
        Some((_center, extent)) => fit_all_zoom(extent).max(min_zoom),
        None => 1.0,
    };
    (min_zoom, max_zoom)
}

fn clamp_pan(rooms: &Query<&RoomSprite>, state: &mut ResMut<CameraState>) {
    let Some((_center, extent)) = compute_dungeon_bounds(rooms) else {
        return;
    };
    let max_pan = extent * 1.5;
    state.offset = state.offset.clamp(-max_pan, max_pan);
}
