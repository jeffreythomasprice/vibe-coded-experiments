use bevy::prelude::*;
use tracing_subscriber::EnvFilter;

mod dungeon;
mod generator;
mod loader;
mod rendering;
mod room_queue;
mod schema_types;

use dungeon::Dungeon;
use rendering::GhostPlacement;
use room_queue::RoomQueue;
use schema_types::Room;

#[derive(States, Default, Debug, Clone, PartialEq, Eq, Hash)]
enum GameState {
    #[default]
    Idle,
    Placing,
}

#[derive(Resource)]
struct PendingRoom(Room);

#[derive(Component)]
struct RoomCountText;

#[derive(Component)]
struct NextRoomButton;

fn main() {
    let pkg = env!("CARGO_PKG_NAME").replace('-', "_");
    let default_filter = format!("{pkg}=trace,warn");

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&default_filter)),
        )
        .init();

    let config = loader::load_config("config.yaml").expect("Failed to load config");
    let themes = loader::load_theme("assets/theme.yaml").expect("Failed to load theme");

    let queue = RoomQueue::new(config, themes, 3);

    App::new()
        .add_plugins(DefaultPlugins)
        .init_state::<GameState>()
        .insert_resource(queue)
        .insert_resource(Dungeon::default())
        .add_systems(Startup, setup_ui)
        .add_systems(Update, (update_room_count, rendering::fit_camera))
        .add_systems(
            Update,
            (auto_place_first_room, handle_button).run_if(in_state(GameState::Idle)),
        )
        .add_systems(
            Update,
            handle_ghost_click.run_if(in_state(GameState::Placing)),
        )
        .add_systems(OnEnter(GameState::Placing), spawn_ghosts)
        .add_systems(OnExit(GameState::Placing), cleanup_ghosts)
        .run();
}

fn setup_ui(mut commands: Commands) {
    commands.spawn(Camera2d);

    commands
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::FlexStart,
            padding: UiRect::all(Val::Px(20.0)),
            row_gap: Val::Px(12.0),
            position_type: PositionType::Absolute,
            ..default()
        })
        .with_children(|parent| {
            parent.spawn((
                Text::new("Rooms available: 0"),
                TextFont {
                    font_size: 28.0,
                    ..default()
                },
                RoomCountText,
            ));

            parent
                .spawn((
                    Button,
                    Node {
                        padding: UiRect::axes(Val::Px(20.0), Val::Px(10.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.3, 0.3, 0.8)),
                    NextRoomButton,
                ))
                .with_child((
                    Text::new("Next Room"),
                    TextFont {
                        font_size: 22.0,
                        ..default()
                    },
                    TextColor(Color::WHITE),
                ));
        });
}

fn update_room_count(queue: Res<RoomQueue>, mut query: Query<&mut Text, With<RoomCountText>>) {
    let count = queue.len();
    for mut text in &mut query {
        **text = format!("Rooms available: {count}");
    }
}

fn auto_place_first_room(
    mut commands: Commands,
    queue: Res<RoomQueue>,
    mut dungeon: ResMut<Dungeon>,
) {
    if !dungeon.grid.is_empty() {
        return;
    }

    if let Some(room) = queue.try_pop() {
        dungeon.place(IVec2::ZERO, room.clone(), 0);
        let placed = &dungeon.grid[&IVec2::ZERO];
        rendering::spawn_room_visuals(&mut commands, IVec2::ZERO, &placed.doors);
    }
}

fn handle_button(
    mut commands: Commands,
    queue: Res<RoomQueue>,
    dungeon: Res<Dungeon>,
    mut next_state: ResMut<NextState<GameState>>,
    mut interaction_query: Query<
        (&Interaction, &mut BackgroundColor),
        (Changed<Interaction>, With<NextRoomButton>),
    >,
) {
    for (interaction, mut bg) in &mut interaction_query {
        match *interaction {
            Interaction::Pressed => {
                if dungeon.grid.is_empty() {
                    continue;
                }
                if let Some(room) = queue.try_pop() {
                    commands.insert_resource(PendingRoom(room));
                    next_state.set(GameState::Placing);
                }
                *bg = BackgroundColor(Color::srgb(0.2, 0.2, 0.6));
            }
            Interaction::Hovered => {
                *bg = BackgroundColor(Color::srgb(0.4, 0.4, 0.9));
            }
            Interaction::None => {
                let color = if queue.len() > 0 {
                    Color::srgb(0.3, 0.3, 0.8)
                } else {
                    Color::srgb(0.5, 0.5, 0.5)
                };
                *bg = BackgroundColor(color);
            }
        }
    }
}

fn spawn_ghosts(
    mut commands: Commands,
    pending: Res<PendingRoom>,
    dungeon: Res<Dungeon>,
) {
    let placements = dungeon.valid_placements(&pending.0.door_config.arrangement);
    for (pos, rotations) in placements {
        rendering::spawn_ghost(&mut commands, pos, rotations);
    }
}

fn handle_ghost_click(
    mut commands: Commands,
    windows: Query<&Window>,
    camera_q: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    mouse: Res<ButtonInput<MouseButton>>,
    ghosts: Query<(Entity, &GhostPlacement)>,
    mut dungeon: ResMut<Dungeon>,
    pending: Res<PendingRoom>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }

    let Ok(window) = windows.single() else { return };
    let Some(cursor) = window.cursor_position() else { return };
    let Ok((camera, cam_transform)) = camera_q.single() else { return };
    let Ok(world_pos) = camera.viewport_to_world_2d(cam_transform, cursor) else { return };

    let half = rendering::ROOM_SIZE / 2.0;

    for (_entity, ghost) in &ghosts {
        let center = rendering::grid_to_world(ghost.pos);
        let min = center - Vec2::splat(half);
        let max = center + Vec2::splat(half);

        if world_pos.x >= min.x && world_pos.x <= max.x && world_pos.y >= min.y && world_pos.y <= max.y {
            let rotation = ghost.rotations[0];
            let room = pending.0.clone();
            dungeon.place(ghost.pos, room, rotation);
            let placed = &dungeon.grid[&ghost.pos];
            rendering::spawn_room_visuals(&mut commands, ghost.pos, &placed.doors);
            commands.remove_resource::<PendingRoom>();
            next_state.set(GameState::Idle);
            return;
        }
    }
}

fn cleanup_ghosts(mut commands: Commands, ghosts: Query<Entity, With<GhostPlacement>>) {
    for entity in &ghosts {
        commands.entity(entity).despawn();
    }
}
