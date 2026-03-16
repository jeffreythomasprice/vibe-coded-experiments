use std::collections::VecDeque;

use bevy::prelude::*;

use crate::dungeon::{Direction, Dungeon, rotated_doors, rotation_count};
use crate::rendering::{self, ActiveGhost, CandidateMarker};
use crate::room_queue::RoomQueue;
use crate::types::*;

pub fn enter_revealing(
    mut commands: Commands,
    revealing: Res<RevealingPlayer>,
    reveal_cell: Res<RevealCell>,
    queue: Res<RoomQueue>,
    dungeon: Res<Dungeon>,
    players: Option<Res<Players>>,
    mut next_state: ResMut<NextState<GameState>>,
    status: Query<&Children, With<StatusText>>,
    mut texts: Query<&mut Text>,
) {
    let player_name = players
        .as_ref()
        .and_then(|p| p.0.get(revealing.0))
        .map(|i| i.player.name.as_str())
        .unwrap_or("Unknown");

    if let Ok(children) = status.single() {
        for child in children.iter() {
            if let Ok(mut text) = texts.get_mut(child) {
                **text = format!("{player_name} is revealing a room.");
            }
        }
    }

    let cell = reveal_cell.0;
    let room = queue
        .try_find(|room| {
            let arrangement = &room.door_config.arrangement;
            let count = rotation_count(arrangement);
            (0..count).any(|rot| dungeon.is_safe_placement(cell, arrangement, rot))
        })
        .or_else(|| queue.try_pop());

    if let Some(room) = room {
        commands.insert_resource(PendingRoom(room));
        next_state.set(GameState::Placing);
    } else {
        commands.remove_resource::<RevealingPlayer>();
        commands.remove_resource::<RevealCell>();
        next_state.set(GameState::Moving);
    }
}

pub fn enter_placing(
    mut commands: Commands,
    dungeon: Res<Dungeon>,
    pending: Res<PendingRoom>,
    reveal_cell: Option<Res<RevealCell>>,
    revealing_player: Option<Res<RevealingPlayer>>,
    players: Option<Res<Players>>,
) {
    if let Some(ref cell) = reveal_cell {
        let arrangement = &pending.0.door_config.arrangement;
        let initial_rot = revealing_player
            .as_ref()
            .and_then(|rp| players.as_ref().map(|p| &p.0[rp.0]))
            .and_then(|info| Direction::from_offset(info.location - cell.0))
            .and_then(|entry_dir| dungeon.find_first_valid_rotation(cell.0, arrangement, entry_dir))
            .unwrap_or(0);
        commands.insert_resource(GhostRotation(initial_rot));
        commands.insert_resource(HoveredCandidate(Some(cell.0)));
        let doors = rotated_doors(arrangement, initial_rot);
        let valid = dungeon.is_safe_placement(cell.0, arrangement, initial_rot);
        rendering::spawn_active_ghost(&mut commands, cell.0, &doors, &pending.0.name, valid);
    } else {
        commands.insert_resource(GhostRotation::default());
        commands.insert_resource(HoveredCandidate::default());
        for pos in dungeon.candidate_cells() {
            rendering::spawn_candidate_marker(&mut commands, pos);
        }
    }
}

pub fn update_ghost_preview(
    mut commands: Commands,
    windows: Query<&Window>,
    camera_q: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    candidates: Query<&CandidateMarker>,
    active_ghosts: Query<Entity, With<ActiveGhost>>,
    mut hovered: ResMut<HoveredCandidate>,
    mut rotation: ResMut<GhostRotation>,
    pending: Res<PendingRoom>,
    dungeon: Res<Dungeon>,
    reveal_cell: Option<Res<RevealCell>>,
) {
    if let Some(ref cell) = reveal_cell {
        if !rotation.is_changed() {
            return;
        }
        for e in &active_ghosts {
            commands.entity(e).despawn();
        }
        let pos = cell.0;
        let arrangement = &pending.0.door_config.arrangement;
        let rot = rotation.0;
        let doors = rotated_doors(arrangement, rot);
        let valid = dungeon.is_safe_placement(pos, arrangement, rot);
        rendering::spawn_active_ghost(&mut commands, pos, &doors, &pending.0.name, valid);
        return;
    }

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

    if found != hovered.0 {
        if let Some(pos) = found {
            let arrangement = &pending.0.door_config.arrangement;
            if !dungeon.is_safe_placement(pos, arrangement, rotation.0) {
                if let Some(valid_rot) = dungeon.find_next_valid_rotation(pos, arrangement, rotation.0) {
                    rotation.0 = valid_rot;
                }
            }
        }
    }

    for e in &active_ghosts {
        commands.entity(e).despawn();
    }

    hovered.0 = found;

    if let Some(pos) = found {
        let arrangement = &pending.0.door_config.arrangement;
        let rot = rotation.0;
        let doors = rotated_doors(arrangement, rot);
        let valid = dungeon.is_safe_placement(pos, arrangement, rot);
        rendering::spawn_active_ghost(&mut commands, pos, &doors, &pending.0.name, valid);
    }
}

pub fn handle_rotation(
    keys: Res<ButtonInput<KeyCode>>,
    mut scroll_events: EventReader<bevy::input::mouse::MouseWheel>,
    pending: Res<PendingRoom>,
    mut rotation: ResMut<GhostRotation>,
    hovered: Res<HoveredCandidate>,
    dungeon: Res<Dungeon>,
) {
    let arrangement = &pending.0.door_config.arrangement;
    let count = rotation_count(arrangement);

    let advance = |current: u8| -> u8 {
        let next = (current + 1) % count;
        match hovered.0 {
            Some(pos) => dungeon
                .find_next_valid_rotation(pos, arrangement, current)
                .unwrap_or(next),
            None => next,
        }
    };

    let retreat = |current: u8| -> u8 {
        let prev = (current + count - 1) % count;
        match hovered.0 {
            Some(pos) => {
                for i in 1..=count {
                    let rot = (current + count - i) % count;
                    if dungeon.is_safe_placement(pos, arrangement, rot) {
                        return rot;
                    }
                }
                prev
            }
            None => prev,
        }
    };

    if keys.just_pressed(KeyCode::KeyR) {
        rotation.0 = advance(rotation.0);
    }
    for ev in scroll_events.read() {
        if ev.y > 0.0 {
            rotation.0 = advance(rotation.0);
        } else if ev.y < 0.0 {
            rotation.0 = retreat(rotation.0);
        }
    }
}

pub fn handle_ghost_click(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    hovered: Res<HoveredCandidate>,
    rotation: Res<GhostRotation>,
    pending: Res<PendingRoom>,
    mut dungeon: ResMut<Dungeon>,
    mut next_state: ResMut<NextState<GameState>>,
    revealing: Option<Res<RevealingPlayer>>,
    _reveal_cell: Option<Res<RevealCell>>,
    mut players: Option<ResMut<Players>>,
    status: Query<&Children, With<StatusText>>,
    mut texts: Query<&mut Text>,
) {
    if !mouse.just_pressed(MouseButton::Left) && !keys.just_pressed(KeyCode::Enter) {
        return;
    }

    let Some(pos) = hovered.0 else { return };
    let arrangement = &pending.0.door_config.arrangement;

    if !dungeon.is_safe_placement(pos, arrangement, rotation.0) {
        return;
    }

    let room = pending.0.clone();
    dungeon.place(pos, room, rotation.0);
    let placed = &dungeon.grid[&pos];
    rendering::spawn_room_visuals(&mut commands, pos, &placed.doors, &placed.room.name);
    commands.remove_resource::<PendingRoom>();

    if let Some(revealing) = revealing {
        let revealer_idx = revealing.0;
        let mut effect_logs = Vec::new();
        let mut allocations = VecDeque::new();

        if let Some(ref mut players) = players {
            if let Some(info) = players.0.get_mut(revealer_idx) {
                info.location = pos;
                info.remaining_move = 0;
                info.path.clear();
                info.destination = None;
            }

            if let Some(placed) = dungeon.grid.get_mut(&pos) {
                let room_name = placed.room.name.clone();
                match placed.room.content.take() {
                    Some(crate::schema_types::RoomContent::Item(item)) => {
                        tracing::info!("Room '{}' at ({},{}) — picking up item: {:?}", room_name, pos.x, pos.y, item);
                        let mut rng = rand::rngs::ThreadRng::default();
                        for effect in &item.effects {
                            let logs = crate::effects::apply_effect(
                                effect, revealer_idx, pos, &mut players.0, &dungeon, &mut rng, crate::effects::EffectSourceKind::Item, &item.name, &item.description, &mut allocations,
                            );
                            effect_logs.extend(logs);
                        }
                        let has_ongoing = item.effects.iter().any(|e| {
                            matches!(e.timing_type, crate::schema_types::EffectTimingType::Duration | crate::schema_types::EffectTimingType::Delayed)
                        });
                        if has_ongoing {
                            if let Some(info) = players.0.get_mut(revealer_idx) {
                                info.inventory.push(item);
                            }
                        }
                    }
                    Some(crate::schema_types::RoomContent::Event(event)) => {
                        tracing::info!("Room '{}' at ({},{}) — triggering event: {:?}", room_name, pos.x, pos.y, event);
                        let mut rng = rand::rngs::ThreadRng::default();
                        for effect in &event.effects {
                            let logs = crate::effects::apply_effect(
                                effect, revealer_idx, pos, &mut players.0, &dungeon, &mut rng, crate::effects::EffectSourceKind::Event, &event.name, &event.description, &mut allocations,
                            );
                            effect_logs.extend(logs);
                        }
                    }
                    None => {}
                }
            }
        }

        commands.remove_resource::<RevealingPlayer>();
        commands.remove_resource::<RevealCell>();

        if let Ok(children) = status.single() {
            for child in children.iter() {
                if let Ok(mut text) = texts.get_mut(child) {
                    **text = String::new();
                }
            }
        }

        if !allocations.is_empty() {
            if !effect_logs.is_empty() {
                commands.insert_resource(EffectLogBuffer(effect_logs));
            }
            commands.insert_resource(DamageAllocationQueue(allocations));
            commands.insert_resource(ResumeState(GameState::Moving));
            next_state.set(GameState::AllocatingDamage);
        } else if !effect_logs.is_empty() {
            commands.insert_resource(EffectLogBuffer(effect_logs));
            commands.insert_resource(ResumeState(GameState::Moving));
            next_state.set(GameState::ShowingEffectLog);
        } else {
            next_state.set(GameState::Moving);
        }
    } else {
        next_state.set(GameState::WaitingForSetup);
    }
}

pub fn exit_placing(
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
