use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use tracing_subscriber::EnvFilter;

mod dungeon;
mod generator;
mod loader;
mod rendering;
mod room_queue;
mod schema_types;

use dungeon::{Dungeon, rotated_doors, rotation_count};
use rendering::{ActiveGhost, CameraState, CandidateMarker, DragState, PlayerDot, RoomSprite};
use room_queue::RoomQueue;
use schema_types::{Player, Room};

#[derive(Resource, Default)]
struct HoveredRoom(Option<IVec2>);

#[derive(Resource, Default)]
struct GhostRotation(u8);

#[derive(Resource, Default)]
struct HoveredCandidate(Option<IVec2>);

#[derive(Component)]
struct SidebarPanel;

#[derive(Component)]
struct SidebarTitle;

#[derive(Component)]
struct SidebarDescription;

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

#[derive(Component)]
struct ResetViewButton;

const PLAYER_COLORS: [Color; 4] = [
    Color::srgb(0.9, 0.2, 0.2),
    Color::srgb(0.2, 0.4, 0.9),
    Color::srgb(0.9, 0.6, 0.1),
    Color::srgb(0.7, 0.3, 0.9),
];

struct PlayerInfo {
    player: Player,
    color: Color,
    location: IVec2,
}

#[derive(Resource)]
struct Players(Vec<PlayerInfo>);

#[derive(Resource)]
struct PendingPlayers(Arc<Mutex<Option<Vec<Player>>>>);

#[derive(Component)]
struct PlayerHudSlot(usize);

#[derive(Component)]
struct PlayerHudName(usize);

#[derive(Component)]
struct SidebarPlayers;

#[derive(Resource, Default)]
struct HoveredPlayer(Option<usize>);

#[derive(Component)]
struct PlayerSidebarPanel;

#[derive(Component)]
struct PlayerSidebarName;

#[derive(Component)]
struct PlayerSidebarDescription;

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

    let queue = RoomQueue::new(config.clone(), themes.clone(), 3);

    let pending_players = Arc::new(Mutex::new(None));
    let pending_clone = pending_players.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
        use rand::SeedableRng;
        let mut rng = rand::rngs::StdRng::from_os_rng();
        rt.block_on(async {
            let mut players = Vec::new();
            for _ in 0..4 {
                match generator::generate_player(&mut rng, &themes, &config).await {
                    Ok(p) => {
                        tracing::info!("Generated player: {}", p.name);
                        players.push(p);
                    }
                    Err(e) => tracing::error!("Player generation failed: {e}"),
                }
            }
            *pending_clone.lock().unwrap() = Some(players);
        });
    });

    App::new()
        .add_plugins(DefaultPlugins)
        .init_state::<GameState>()
        .insert_resource(queue)
        .insert_resource(Dungeon::default())
        .insert_resource(CameraState { zoom: 1.0, offset: Vec2::ZERO })
        .insert_resource(DragState::default())
        .insert_resource(HoveredRoom::default())
        .insert_resource(HoveredPlayer::default())
        .insert_resource(PendingPlayers(pending_players))
        .add_systems(Startup, setup_ui)
        .add_systems(
            Update,
            (
                update_room_count,
                rendering::camera_keyboard_pan,
                rendering::camera_zoom,
                rendering::camera_drag_pan,
                rendering::apply_camera,
                handle_reset_view,
                detect_hovered_room,
                update_sidebar,
                check_pending_players,
                update_player_hud,
                update_player_dots,
                detect_player_hover,
                update_player_sidebar,
            ),
        )
        .add_systems(
            Update,
            (auto_place_first_room, handle_button).run_if(in_state(GameState::Idle)),
        )
        .add_systems(
            Update,
            (update_ghost_preview, handle_rotation, handle_ghost_click)
                .run_if(in_state(GameState::Placing)),
        )
        .add_systems(OnEnter(GameState::Placing), enter_placing)
        .add_systems(OnExit(GameState::Placing), exit_placing)
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

            parent
                .spawn((
                    Button,
                    Node {
                        padding: UiRect::axes(Val::Px(20.0), Val::Px(10.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.5, 0.5, 0.5)),
                    ResetViewButton,
                ))
                .with_child((
                    Text::new("Reset View"),
                    TextFont {
                        font_size: 22.0,
                        ..default()
                    },
                    TextColor(Color::WHITE),
                ));
        });

    // Player HUD (top center)
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            top: Val::Px(10.0),
            width: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(16.0),
            ..default()
        })
        .with_children(|parent| {
            for i in 0..4 {
                parent
                    .spawn((
                        Button,
                        Node {
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            row_gap: Val::Px(4.0),
                            ..default()
                        },
                        PlayerHudSlot(i),
                    ))
                    .with_children(|slot| {
                        slot.spawn((
                            Node {
                                width: Val::Px(24.0),
                                height: Val::Px(24.0),
                                border: UiRect::all(Val::Px(2.0)),
                                ..default()
                            },
                            BackgroundColor(PLAYER_COLORS[i]),
                            BorderColor(Color::WHITE),
                        ));
                        slot.spawn((
                            Text::new("..."),
                            TextFont { font_size: 12.0, ..default() },
                            TextColor(Color::WHITE),
                            PlayerHudName(i),
                        ));
                    });
            }
        });

    // Right sidebar for room info on hover
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                width: Val::Px(250.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(16.0)),
                row_gap: Val::Px(12.0),
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::srgba(0.1, 0.1, 0.15, 0.85)),
            SidebarPanel,
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new(""),
                TextFont {
                    font_size: 22.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                SidebarTitle,
            ));
            parent.spawn((
                Text::new(""),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
                TextColor(Color::srgb(0.8, 0.8, 0.8)),
                SidebarDescription,
            ));
            parent.spawn((
                Text::new(""),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
                TextColor(Color::srgb(0.9, 0.85, 0.5)),
                SidebarPlayers,
            ));
        });

    // Left sidebar for player info on hover
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                width: Val::Px(250.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(16.0)),
                row_gap: Val::Px(12.0),
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::srgba(0.1, 0.1, 0.15, 0.85)),
            PlayerSidebarPanel,
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new(""),
                TextFont { font_size: 22.0, ..default() },
                TextColor(Color::WHITE),
                PlayerSidebarName,
            ));
            parent.spawn((
                Text::new(""),
                TextFont { font_size: 14.0, ..default() },
                TextColor(Color::srgb(0.8, 0.8, 0.8)),
                PlayerSidebarDescription,
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
        rendering::spawn_room_visuals(&mut commands, IVec2::ZERO, &placed.doors, &placed.room.name);
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

fn enter_placing(
    mut commands: Commands,
    dungeon: Res<Dungeon>,
) {
    commands.insert_resource(GhostRotation::default());
    commands.insert_resource(HoveredCandidate::default());

    for pos in dungeon.candidate_cells() {
        rendering::spawn_candidate_marker(&mut commands, pos);
    }
}

fn update_ghost_preview(
    mut commands: Commands,
    windows: Query<&Window>,
    camera_q: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    candidates: Query<&CandidateMarker>,
    active_ghosts: Query<Entity, With<ActiveGhost>>,
    mut hovered: ResMut<HoveredCandidate>,
    rotation: Res<GhostRotation>,
    pending: Res<PendingRoom>,
    dungeon: Res<Dungeon>,
) {
    let Ok(window) = windows.single() else { return };
    let Some(cursor) = window.cursor_position() else {
        if hovered.0.is_some() {
            hovered.0 = None;
            for e in &active_ghosts {
                commands.entity(e).despawn();
            }
        }
        return;
    };
    let Ok((camera, cam_transform)) = camera_q.single() else { return };
    let Ok(world_pos) = camera.viewport_to_world_2d(cam_transform, cursor) else { return };

    let half = rendering::ROOM_SIZE / 2.0;
    let mut found = None;
    for marker in &candidates {
        let center = rendering::grid_to_world(marker.0);
        if (world_pos.x - center.x).abs() <= half && (world_pos.y - center.y).abs() <= half {
            found = Some(marker.0);
            break;
        }
    }

    if found == hovered.0 && !rotation.is_changed() {
        return;
    }

    // despawn old ghost
    for e in &active_ghosts {
        commands.entity(e).despawn();
    }

    hovered.0 = found;

    if let Some(pos) = found {
        let arrangement = &pending.0.door_config.arrangement;
        let rot = rotation.0;
        let doors = rotated_doors(arrangement, rot);
        let valid = dungeon.is_valid_placement(pos, arrangement, rot);
        rendering::spawn_active_ghost(&mut commands, pos, &doors, &pending.0.name, valid);
    }
}

fn handle_rotation(
    keys: Res<ButtonInput<KeyCode>>,
    pending: Res<PendingRoom>,
    mut rotation: ResMut<GhostRotation>,
) {
    if keys.just_pressed(KeyCode::KeyR) {
        let count = rotation_count(&pending.0.door_config.arrangement);
        rotation.0 = (rotation.0 + 1) % count;
    }
}

fn handle_ghost_click(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    hovered: Res<HoveredCandidate>,
    rotation: Res<GhostRotation>,
    pending: Res<PendingRoom>,
    mut dungeon: ResMut<Dungeon>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }

    let Some(pos) = hovered.0 else { return };
    let arrangement = &pending.0.door_config.arrangement;

    if !dungeon.is_valid_placement(pos, arrangement, rotation.0) {
        return;
    }

    let room = pending.0.clone();
    dungeon.place(pos, room, rotation.0);
    let placed = &dungeon.grid[&pos];
    rendering::spawn_room_visuals(&mut commands, pos, &placed.doors, &placed.room.name);
    commands.remove_resource::<PendingRoom>();
    next_state.set(GameState::Idle);
}

fn handle_reset_view(
    rooms: Query<&rendering::RoomSprite>,
    mut state: ResMut<CameraState>,
    mut interaction_query: Query<
        (&Interaction, &mut BackgroundColor),
        (Changed<Interaction>, With<ResetViewButton>),
    >,
) {
    for (interaction, mut bg) in &mut interaction_query {
        match *interaction {
            Interaction::Pressed => {
                state.offset = Vec2::ZERO;
                if let Some((_center, extent)) = rendering::compute_dungeon_bounds(&rooms) {
                    state.zoom = rendering::fit_all_zoom(extent);
                }
                *bg = BackgroundColor(Color::srgb(0.35, 0.35, 0.35));
            }
            Interaction::Hovered => {
                *bg = BackgroundColor(Color::srgb(0.6, 0.6, 0.6));
            }
            Interaction::None => {
                *bg = BackgroundColor(Color::srgb(0.5, 0.5, 0.5));
            }
        }
    }
}

fn detect_hovered_room(
    windows: Query<&Window>,
    camera_q: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    rooms: Query<&RoomSprite>,
    mut hovered: ResMut<HoveredRoom>,
) {
    let Ok(window) = windows.single() else { return };
    let Some(cursor) = window.cursor_position() else {
        hovered.0 = None;
        return;
    };
    let Ok((camera, cam_transform)) = camera_q.single() else { return };
    let Ok(world_pos) = camera.viewport_to_world_2d(cam_transform, cursor) else { return };

    let half = rendering::ROOM_SIZE / 2.0;
    let mut found = None;
    for room in &rooms {
        let center = rendering::grid_to_world(room.0);
        if (world_pos.x - center.x).abs() <= half && (world_pos.y - center.y).abs() <= half {
            found = Some(room.0);
            break;
        }
    }
    hovered.0 = found;
}

fn update_sidebar(
    hovered: Res<HoveredRoom>,
    dungeon: Res<Dungeon>,
    players: Option<Res<Players>>,
    mut panel: Query<&mut Node, With<SidebarPanel>>,
    mut title: Query<&mut Text, With<SidebarTitle>>,
    mut description: Query<&mut Text, (With<SidebarDescription>, Without<SidebarTitle>, Without<SidebarPlayers>)>,
    mut players_text: Query<&mut Text, (With<SidebarPlayers>, Without<SidebarTitle>, Without<SidebarDescription>)>,
) {
    if !hovered.is_changed() {
        return;
    }
    let Ok(mut panel_node) = panel.single_mut() else { return };
    let Ok(mut title_text) = title.single_mut() else { return };
    let Ok(mut desc_text) = description.single_mut() else { return };
    let Ok(mut pt) = players_text.single_mut() else { return };

    match hovered.0 {
        Some(pos) => {
            if let Some(placed) = dungeon.grid.get(&pos) {
                **title_text = placed.room.name.clone();
                **desc_text = placed.room.description.clone();

                let player_names: Vec<&str> = players
                    .as_ref()
                    .map(|p| {
                        p.0.iter()
                            .filter(|info| info.location == pos)
                            .map(|info| info.player.name.as_str())
                            .collect()
                    })
                    .unwrap_or_default();

                if player_names.is_empty() {
                    **pt = String::new();
                } else {
                    **pt = format!("Players: {}", player_names.join(", "));
                }

                panel_node.display = Display::Flex;
            }
        }
        None => {
            panel_node.display = Display::None;
        }
    }
}

fn check_pending_players(
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
            })
            .collect();
        commands.insert_resource(Players(infos));
        commands.remove_resource::<PendingPlayers>();
    }
}

fn update_player_hud(
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

fn update_player_dots(
    mut commands: Commands,
    players: Option<Res<Players>>,
    existing_dots: Query<Entity, With<PlayerDot>>,
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
            Sprite {
                color: info.color,
                custom_size: Some(Vec2::splat(dot_size)),
                ..default()
            },
            Transform::from_translation(pos),
            PlayerDot(i),
        ));
    }
}

fn detect_player_hover(
    slots: Query<(&Interaction, &PlayerHudSlot), Changed<Interaction>>,
    mut hovered: ResMut<HoveredPlayer>,
) {
    for (interaction, slot) in &slots {
        match *interaction {
            Interaction::Hovered | Interaction::Pressed => {
                hovered.0 = Some(slot.0);
                return;
            }
            Interaction::None => {
                if hovered.0 == Some(slot.0) {
                    hovered.0 = None;
                }
            }
        }
    }
}

fn update_player_sidebar(
    hovered: Res<HoveredPlayer>,
    players: Option<Res<Players>>,
    mut panel: Query<&mut Node, With<PlayerSidebarPanel>>,
    mut name: Query<&mut Text, With<PlayerSidebarName>>,
    mut desc: Query<&mut Text, (With<PlayerSidebarDescription>, Without<PlayerSidebarName>)>,
) {
    if !hovered.is_changed() {
        return;
    }
    let Ok(mut panel_node) = panel.single_mut() else { return };
    let Ok(mut name_text) = name.single_mut() else { return };
    let Ok(mut desc_text) = desc.single_mut() else { return };

    match hovered.0 {
        Some(idx) => {
            if let Some(info) = players.as_ref().and_then(|p| p.0.get(idx)) {
                **name_text = info.player.name.clone();
                **desc_text = info.player.description.clone();
                panel_node.display = Display::Flex;
            }
        }
        None => {
            panel_node.display = Display::None;
        }
    }
}

fn exit_placing(
    mut commands: Commands,
    markers: Query<Entity, With<CandidateMarker>>,
    ghosts: Query<Entity, With<ActiveGhost>>,
) {
    for entity in markers.iter().chain(ghosts.iter()) {
        commands.entity(entity).despawn();
    }
    commands.remove_resource::<GhostRotation>();
    commands.remove_resource::<HoveredCandidate>();
}
