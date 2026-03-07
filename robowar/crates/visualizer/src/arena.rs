use bevy::prelude::*;

use robowar_shared::sim::arena::ArenaBody;

use crate::simulation::{MatchSetup, SimConfig};

#[derive(Component)]
pub struct ArenaEntity;

#[derive(Resource)]
pub struct CameraState {
    pub arena_width: f32,
    pub arena_height: f32,
    pub robot_radius: f32,
    pub pan_speed: f32,
    pub last_cursor_pos: Option<Vec2>,
}

const ARENA_BG_COLOR: Color = Color::srgb(0.1, 0.1, 0.15);
const WALL_COLOR: Color = Color::srgb(0.3, 0.3, 0.3);
const OBSTACLE_COLOR: Color = Color::srgb(0.4, 0.4, 0.4);

pub fn spawn_camera(
    mut commands: Commands,
    sim_config: Res<SimConfig>,
    setup: Res<MatchSetup>,
    windows: Query<&Window>,
) {
    let (w, h) = setup.config.arena.bounds;

    let window = windows.single();
    let window_w = window.resolution.width();
    let window_h = window.resolution.height();
    let scale = (w / window_w).max(h / window_h);

    let mut projection = OrthographicProjection::default_2d();
    projection.scale = scale;

    let transform = Transform::from_xyz(w / 2.0, h / 2.0, 999.0);
    commands.spawn((Camera2d, projection, transform));

    commands.insert_resource(CameraState {
        arena_width: w,
        arena_height: h,
        robot_radius: sim_config.constants.robot_radius,
        pan_speed: sim_config.constants.camera_pan_speed,
        last_cursor_pos: None,
    });
}

pub fn spawn_arena(mut commands: Commands, setup: Res<MatchSetup>) {
    let arena = &setup.config.arena;
    let (arena_w, arena_h) = arena.bounds;

    commands.spawn((
        ArenaEntity,
        Sprite {
            color: ARENA_BG_COLOR,
            custom_size: Some(Vec2::new(arena_w, arena_h)),
            ..default()
        },
        Transform::from_xyz(arena_w / 2.0, arena_h / 2.0, 0.0),
    ));

    for body in &arena.bodies {
        match body {
            ArenaBody::Rectangle {
                x,
                y,
                width,
                height,
                rotation,
            } => {
                commands.spawn((
                    ArenaEntity,
                    Sprite {
                        color: WALL_COLOR,
                        custom_size: Some(Vec2::new(*width, *height)),
                        ..default()
                    },
                    Transform::from_xyz(*x, *y, 1.0)
                        .with_rotation(Quat::from_rotation_z(*rotation)),
                ));
            }
            ArenaBody::Circle { x, y, radius } => {
                commands.spawn((
                    ArenaEntity,
                    Sprite {
                        color: OBSTACLE_COLOR,
                        custom_size: Some(Vec2::new(*radius * 2.0, *radius * 2.0)),
                        ..default()
                    },
                    Transform::from_xyz(*x, *y, 1.0),
                ));
            }
        }
    }
}
