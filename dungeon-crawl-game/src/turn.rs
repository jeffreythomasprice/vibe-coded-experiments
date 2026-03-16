use std::collections::VecDeque;

use bevy::prelude::*;
use rand::Rng;

use crate::dungeon::{self, Dungeon};
use crate::rendering;
use crate::room_queue::RoomQueue;
use crate::types::*;

pub fn auto_place_first_room(
    mut commands: Commands,
    queue: Res<RoomQueue>,
    mut dungeon: ResMut<Dungeon>,
    placeholders: Query<Entity, With<PlaceholderRoom>>,
) {
    if !dungeon.grid.is_empty() {
        return;
    }

    if let Some(room) = queue.try_pop() {
        for entity in &placeholders {
            commands.entity(entity).despawn();
        }
        dungeon.place(IVec2::ZERO, room.clone(), 0);
        let placed = &dungeon.grid[&IVec2::ZERO];
        rendering::spawn_room_visuals(&mut commands, IVec2::ZERO, &placed.doors, &placed.room.name);
    }
}

pub fn auto_start_first_turn(
    players: Option<Res<Players>>,
    dungeon: Res<Dungeon>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    if !dungeon.grid.is_empty() && players.is_some() {
        next_state.set(GameState::StartingTurn);
    }
}

pub fn enter_starting_turn(
    mut commands: Commands,
    mut next_state: ResMut<NextState<GameState>>,
    mut players: Option<ResMut<Players>>,
    dungeon: Res<Dungeon>,
    mut log_buffer: ResMut<EffectLogBuffer>,
    progress: Option<Res<TickEffectsProgress>>,
) {
    let Some(ref mut players) = players else {
        next_state.set(GameState::SelectingDestinations);
        return;
    };

    let start = progress.as_ref().map(|p| p.0).unwrap_or(0);
    let mut rng = rand::rngs::ThreadRng::default();

    for idx in start..players.0.len() {
        let mut allocations = std::collections::VecDeque::new();
        let logs = crate::effects::tick_effects_for_player(idx, &mut players.0, &dungeon, &mut rng, &mut allocations);
        for log in &logs {
            tracing::info!("Effect tick: {}", log.description);
        }
        if !allocations.is_empty() {
            if !logs.is_empty() {
                log_buffer.0.extend(logs);
            }
            commands.insert_resource(DamageAllocationQueue(allocations));
            commands.insert_resource(TickEffectsProgress(idx + 1));
            commands.insert_resource(ResumeState(GameState::StartingTurn));
            next_state.set(GameState::AllocatingDamage);
            return;
        }
        if !logs.is_empty() {
            log_buffer.0 = logs;
            commands.insert_resource(TickEffectsProgress(idx + 1));
            commands.insert_resource(ResumeState(GameState::StartingTurn));
            next_state.set(GameState::ShowingEffectLog);
            return;
        }
    }

    commands.remove_resource::<TickEffectsProgress>();
    next_state.set(GameState::SelectingDestinations);
}

pub fn enter_selecting(
    reselecting: Option<Res<Reselecting>>,
    mut turn: ResMut<TurnNumber>,
    mut players: Option<ResMut<Players>>,
    mut selected: ResMut<SelectedPlayer>,
    status: Query<&Children, With<StatusText>>,
    mut texts: Query<&mut Text>,
) {
    if reselecting.is_some() { return; }

    turn.0 += 1;
    selected.0 = None;

    if let Some(ref mut players) = players {
        for info in &mut players.0 {
            if info.dead {
                info.remaining_move = 0;
            } else {
                info.remaining_move = (info.player.stats.speed + 1) / 2;
            }
            info.destination = None;
            info.path.clear();
        }
    }

    if let Ok(children) = status.single() {
        for child in children.iter() {
            if let Ok(mut text) = texts.get_mut(child) {
                **text = "Select destinations for each player".to_string();
            }
        }
    }
}

pub fn enter_reselecting(
    mut commands: Commands,
    reselecting: Option<Res<Reselecting>>,
    mut players: Option<ResMut<Players>>,
    mut selected: ResMut<SelectedPlayer>,
    status: Query<&Children, With<StatusText>>,
    mut texts: Query<&mut Text>,
) {
    if reselecting.is_none() { return; }
    commands.remove_resource::<Reselecting>();
    selected.0 = None;

    if let Some(ref mut players) = players {
        for info in &mut players.0 {
            if info.remaining_move > 0 {
                info.destination = None;
                info.path.clear();
            }
        }
    }

    if let Ok(children) = status.single() {
        for child in children.iter() {
            if let Ok(mut text) = texts.get_mut(child) {
                **text = "Select destinations for players with remaining movement".to_string();
            }
        }
    }
}

pub fn exit_selecting(
    mut commands: Commands,
    reachable: Query<Entity, With<ReachableMarker>>,
    previews: Query<Entity, With<PathPreviewMarker>>,
    arrows: Query<Entity, With<DestinationArrow>>,
    status: Query<&Children, With<StatusText>>,
    mut texts: Query<&mut Text>,
) {
    for entity in reachable.iter().chain(previews.iter()).chain(arrows.iter()) {
        commands.entity(entity).despawn();
    }
    if let Ok(children) = status.single() {
        for child in children.iter() {
            if let Ok(mut text) = texts.get_mut(child) {
                **text = String::new();
            }
        }
    }
}

pub fn handle_player_selection(
    mut selected: ResMut<SelectedPlayer>,
    slots: Query<(&Interaction, &PlayerHudSlot), Changed<Interaction>>,
    keys: Res<ButtonInput<KeyCode>>,
    players: Option<Res<Players>>,
) {
    for (interaction, slot) in &slots {
        if *interaction == Interaction::Pressed {
            selected.0 = Some(slot.0);
        }
    }

    let count = players.map(|p| p.0.len()).unwrap_or(0);
    let number_keys = [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4];
    for (i, key) in number_keys.iter().enumerate() {
        if keys.just_pressed(*key) && i < count {
            selected.0 = Some(i);
        }
    }
}

pub fn update_reachable_highlights(
    mut commands: Commands,
    selected: Res<SelectedPlayer>,
    players: Option<Res<Players>>,
    dungeon: Res<Dungeon>,
    existing: Query<Entity, With<ReachableMarker>>,
    previews: Query<Entity, With<PathPreviewMarker>>,
) {
    if !selected.is_changed() { return; }

    for entity in existing.iter().chain(previews.iter()) {
        commands.entity(entity).despawn();
    }

    let Some(idx) = selected.0 else { return };
    let Some(ref players) = players else { return };
    let Some(info) = players.0.get(idx) else { return };

    let reachable = dungeon.reachable_within(info.location, info.remaining_move);
    for pos in &reachable {
        if *pos == info.location { continue; }
        let world = rendering::grid_to_world(*pos);
        let color = if dungeon.grid.contains_key(pos) {
            Color::srgba(0.3, 0.7, 1.0, 0.2)
        } else {
            Color::srgba(0.3, 1.0, 0.5, 0.2)
        };
        commands.spawn((
            Sprite {
                color,
                custom_size: Some(Vec2::splat(rendering::ROOM_SIZE * 0.9)),
                ..default()
            },
            Transform::from_translation(world.extend(-0.5)),
            ReachableMarker,
        ));
    }

    if let Some(_dest) = info.destination {
        for pos in &info.path {
            let world = rendering::grid_to_world(*pos);
            commands.spawn((
                Sprite {
                    color: Color::srgba(1.0, 1.0, 0.3, 0.3),
                    custom_size: Some(Vec2::splat(rendering::ROOM_SIZE * 0.4)),
                    ..default()
                },
                Transform::from_translation(world.extend(0.5)),
                PathPreviewMarker,
            ));
        }
    }
}

pub fn handle_destination_click(
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    camera_q: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    selected: Res<SelectedPlayer>,
    dungeon: Res<Dungeon>,
    mut players: Option<ResMut<Players>>,
    mut commands: Commands,
    previews: Query<Entity, With<PathPreviewMarker>>,
) {
    if !mouse.just_pressed(MouseButton::Left) { return; }
    let Some(idx) = selected.0 else { return; };
    let Some(ref mut players) = players else { return; };
    let Some(info) = players.0.get(idx) else { return; };

    let Ok(window) = windows.single() else { return; };
    let Some(cursor) = window.cursor_position() else { return; };
    let Ok((camera, cam_transform)) = camera_q.single() else { return; };
    let Ok(world_pos) = camera.viewport_to_world_2d(cam_transform, cursor) else { return; };

    let half = rendering::ROOM_SIZE / 2.0;
    let reachable = dungeon.reachable_within(info.location, info.remaining_move);

    let mut clicked_pos = None;
    for pos in &reachable {
        let center = rendering::grid_to_world(*pos);
        if (world_pos.x - center.x).abs() <= half && (world_pos.y - center.y).abs() <= half {
            clicked_pos = Some(*pos);
            break;
        }
    }

    let Some(target) = clicked_pos else { return; };
    if target == info.location { return; }

    let from = info.location;

    let path = if dungeon.grid.contains_key(&target) {
        dungeon.shortest_path(from, target)
    } else {
        let mut best_path: Option<Vec<IVec2>> = None;
        for dir in dungeon::Direction::all() {
            let adj = target + dir.offset();
            if let Some(placed) = dungeon.grid.get(&adj) {
                if placed.doors.contains(&dir.opposite()) {
                    if let Some(mut p) = dungeon.shortest_path(from, adj) {
                        p.push(target);
                        if best_path.as_ref().is_none_or(|bp| p.len() < bp.len()) {
                            best_path = Some(p);
                        }
                    }
                }
            }
        }
        best_path
    };

    let Some(path) = path else { return; };

    for entity in &previews {
        commands.entity(entity).despawn();
    }

    let info = &mut players.0[idx];
    info.destination = Some(target);
    info.path = VecDeque::from(path.clone());
    info.path.pop_front();

    for pos in &path {
        if *pos == from { continue; }
        let world = rendering::grid_to_world(*pos);
        commands.spawn((
            Sprite {
                color: Color::srgba(1.0, 1.0, 0.3, 0.3),
                custom_size: Some(Vec2::splat(rendering::ROOM_SIZE * 0.4)),
                ..default()
            },
            Transform::from_translation(world.extend(0.5)),
            PathPreviewMarker,
        ));
    }
}

pub fn check_all_destinations_set(
    players: Option<Res<Players>>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    let Some(ref players) = players else { return; };
    if players.0.iter().all(|p| p.dead || p.remaining_move <= 0 || p.destination.is_some()) {
        next_state.set(GameState::Moving);
    }
}

pub fn enter_moving(mut commands: Commands) {
    commands.insert_resource(MoveTimer(Timer::from_seconds(0.3, TimerMode::Repeating)));
}

pub fn advance_movement(
    time: Res<Time>,
    mut timer: ResMut<MoveTimer>,
    mut players: Option<ResMut<Players>>,
    dungeon: Res<Dungeon>,
    mut next_state: ResMut<NextState<GameState>>,
    mut commands: Commands,
) {
    timer.0.tick(time.delta());
    if !timer.0.just_finished() { return; }

    let Some(ref mut players) = players else { return; };

    // Collect candidates whose next step is into an unrevealed cell
    let candidates: Vec<(usize, i64, i64)> = players.0.iter().enumerate()
        .filter(|(_, info)| !info.dead && info.remaining_move > 0 && !info.path.is_empty())
        .filter(|(_, info)| !dungeon.grid.contains_key(&info.path[0]))
        .map(|(i, info)| {
            let distance = (info.player.stats.speed + 1) / 2 - info.remaining_move + 1;
            (i, distance, info.player.stats.speed)
        })
        .collect();

    if !candidates.is_empty() {
        let mut rng = rand::rng();
        let &(winner, _, _) = candidates.iter()
            .min_by(|a, b| a.1.cmp(&b.1)
                .then(b.2.cmp(&a.2))
                .then_with(|| if rng.random_bool(0.5) {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                }))
            .unwrap();
        commands.insert_resource(RevealingPlayer(winner));
        commands.insert_resource(RevealCell(players.0[winner].path[0]));
        next_state.set(GameState::RevealingRoom);
        commands.remove_resource::<MoveTimer>();
        return;
    }

    let mut any_moved = false;
    for info in &mut players.0 {
        if info.dead || info.remaining_move <= 0 || info.path.is_empty() { continue; }
        let next = info.path.pop_front().unwrap();
        info.location = next;
        info.remaining_move -= 1;
        any_moved = true;
    }

    if !any_moved {
        commands.remove_resource::<MoveTimer>();
        if players.0.iter().any(|p| p.remaining_move > 0) {
            commands.insert_resource(Reselecting);
            next_state.set(GameState::SelectingDestinations);
        } else {
            next_state.set(GameState::StartingTurn);
        }
    }
}

pub fn handle_end_turn(
    mut players: Option<ResMut<Players>>,
    mut next_state: ResMut<NextState<GameState>>,
    mut commands: Commands,
    state: Res<State<GameState>>,
    mut interaction_query: Query<
        (&Interaction, &mut BackgroundColor),
        (Changed<Interaction>, With<EndTurnButton>),
    >,
) {
    for (interaction, mut bg) in &mut interaction_query {
        match *interaction {
            Interaction::Pressed => {
                if let Some(ref mut players) = players {
                    for info in &mut players.0 {
                        info.remaining_move = 0;
                        info.path.clear();
                        info.destination = None;
                    }
                }
                if *state.get() == GameState::SelectingDestinations {
                    next_state.set(GameState::StartingTurn);
                }
                if *state.get() == GameState::Moving {
                    commands.remove_resource::<MoveTimer>();
                    next_state.set(GameState::StartingTurn);
                }
                *bg = BackgroundColor(Color::srgb(0.6, 0.3, 0.1));
            }
            Interaction::Hovered => {
                *bg = BackgroundColor(Color::srgb(0.9, 0.5, 0.3));
            }
            Interaction::None => {
                *bg = BackgroundColor(Color::srgb(0.8, 0.4, 0.2));
            }
        }
    }
}
