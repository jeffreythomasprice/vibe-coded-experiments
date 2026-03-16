use bevy::prelude::*;

use crate::dungeon::Dungeon;
use crate::rendering::RoomSprite;
use crate::types::*;

pub fn detect_hovered_room(
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

    let half = crate::rendering::ROOM_SIZE / 2.0;
    let mut found = None;
    for room in &rooms {
        let center = crate::rendering::grid_to_world(room.0);
        if (world_pos.x - center.x).abs() <= half && (world_pos.y - center.y).abs() <= half {
            found = Some(room.0);
            break;
        }
    }
    hovered.0 = found;
}

pub fn update_sidebar(
    hovered: Res<HoveredRoom>,
    dungeon: Res<Dungeon>,
    players: Option<Res<Players>>,
    mut panel: Query<&mut Node, With<SidebarPanel>>,
    mut title: Query<&mut Text, With<SidebarTitle>>,
    mut description: Query<&mut Text, (With<SidebarDescription>, Without<SidebarTitle>, Without<SidebarPlayers>, Without<SidebarEffects>)>,
    mut players_text: Query<&mut Text, (With<SidebarPlayers>, Without<SidebarTitle>, Without<SidebarDescription>, Without<SidebarEffects>)>,
    mut effects_text: Query<&mut Text, (With<SidebarEffects>, Without<SidebarTitle>, Without<SidebarDescription>, Without<SidebarPlayers>)>,
) {
    if !hovered.is_changed() {
        return;
    }
    let Ok(mut panel_node) = panel.single_mut() else { return };
    let Ok(mut title_text) = title.single_mut() else { return };
    let Ok(mut desc_text) = description.single_mut() else { return };
    let Ok(mut pt) = players_text.single_mut() else { return };
    let Ok(mut et) = effects_text.single_mut() else { return };

    match hovered.0.and_then(|pos| dungeon.grid.get(&pos).map(|placed| (pos, placed))) {
        Some((pos, placed)) => {
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

            if placed.active_effects.is_empty() {
                **et = String::new();
            } else {
                let descs: Vec<String> = placed.active_effects.iter()
                    .map(|ae| crate::effects::format_effect_description(&ae.effect))
                    .collect();
                **et = format!("Effects: {}", descs.join(", "));
            }

            panel_node.display = Display::Flex;
        }
        None => {
            panel_node.display = Display::None;
        }
    }
}

pub fn detect_player_hover(
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

pub fn update_player_sidebar(
    hovered: Res<HoveredPlayer>,
    players: Option<Res<Players>>,
    mut panel: Query<&mut Node, With<PlayerSidebarPanel>>,
    mut name: Query<&mut Text, With<PlayerSidebarName>>,
    mut desc: Query<&mut Text, (With<PlayerSidebarDescription>, Without<PlayerSidebarName>, Without<PlayerSidebarStats>, Without<PlayerSidebarEffects>, Without<PlayerSidebarInventory>)>,
    mut stats: Query<&mut Text, (With<PlayerSidebarStats>, Without<PlayerSidebarName>, Without<PlayerSidebarDescription>, Without<PlayerSidebarEffects>, Without<PlayerSidebarInventory>)>,
    mut effects: Query<&mut Text, (With<PlayerSidebarEffects>, Without<PlayerSidebarName>, Without<PlayerSidebarDescription>, Without<PlayerSidebarStats>, Without<PlayerSidebarInventory>)>,
    mut inventory: Query<&mut Text, (With<PlayerSidebarInventory>, Without<PlayerSidebarName>, Without<PlayerSidebarDescription>, Without<PlayerSidebarStats>, Without<PlayerSidebarEffects>)>,
) {
    if !hovered.is_changed() {
        return;
    }
    let Ok(mut panel_node) = panel.single_mut() else { return };
    let Ok(mut name_text) = name.single_mut() else { return };
    let Ok(mut desc_text) = desc.single_mut() else { return };
    let Ok(mut stats_text) = stats.single_mut() else { return };
    let Ok(mut effects_text) = effects.single_mut() else { return };
    let Ok(mut inventory_text) = inventory.single_mut() else { return };

    match hovered.0.and_then(|idx| players.as_ref().and_then(|p| p.0.get(idx))) {
        Some(info) => {
            **name_text = if info.dead {
                format!("{} [DEAD]", info.player.name)
            } else {
                info.player.name.clone()
            };
            **desc_text = info.player.description.clone();

            let s = &info.player.stats;
            **stats_text = format!(
                "STR {}\nSPD {}\nINT {}\nSAN {}\nMove {}/turn",
                s.strength, s.speed, s.intelligence, s.sanity, info.remaining_move
            );

            if info.inventory.is_empty() {
                **inventory_text = String::new();
            } else {
                let mut item_effects: std::collections::HashMap<&str, Vec<String>> =
                    std::collections::HashMap::new();
                for ae in &info.active_effects {
                    if ae.source_kind == crate::effects::EffectSourceKind::Item {
                        item_effects.entry(ae.source_name.as_str())
                            .or_default()
                            .push(crate::effects::format_effect_description(&ae.effect));
                    }
                }
                let lines: Vec<String> = info.inventory.iter().map(|item| {
                    match item_effects.get(item.name.as_str()) {
                        Some(effs) => format!("  {} - {}", item.name, effs.join(", ")),
                        None => format!("  {}", item.name),
                    }
                }).collect();
                **inventory_text = format!("Inventory:\n{}", lines.join("\n"));
            }

            let event_effects: Vec<String> = info.active_effects.iter()
                .filter(|ae| ae.source_kind == crate::effects::EffectSourceKind::Event)
                .map(|ae| crate::effects::format_effect_description(&ae.effect))
                .collect();
            if event_effects.is_empty() {
                **effects_text = String::new();
            } else {
                **effects_text = format!("Effects: {}", event_effects.join(", "));
            }

            panel_node.display = Display::Flex;
        }
        None => {
            panel_node.display = Display::None;
        }
    }
}
