use bevy::prelude::*;

use crate::effects;
use crate::rendering;
use crate::room_queue::RoomQueue;
use crate::schema_types::StatName;
use crate::types::*;

pub fn setup_ui(mut commands: Commands) {
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
                Text::new("Turn: 0"),
                TextFont {
                    font_size: 28.0,
                    ..default()
                },
                TurnNumberText,
            ));

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
                        display: Display::None,
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.8, 0.4, 0.2)),
                    EndTurnButton,
                ))
                .with_child((
                    Text::new("End Turn"),
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
                            BorderColor(Color::NONE),
                            PlayerHudColor(i),
                        ));
                        slot.spawn((
                            Text::new("?"),
                            TextFont {
                                font_size: 12.0,
                                ..default()
                            },
                            TextColor(Color::WHITE),
                            PlayerHudName(i),
                        ));
                        slot.spawn((
                            Text::new(""),
                            TextFont {
                                font_size: 9.0,
                                ..default()
                            },
                            TextColor(Color::srgb(0.7, 0.8, 0.9)),
                            PlayerHudStats(i),
                        ));
                        slot.spawn((
                            Text::new(""),
                            TextFont {
                                font_size: 10.0,
                                ..default()
                            },
                            TextColor(Color::srgb(0.7, 0.9, 0.7)),
                            PlayerHudMove(i),
                        ));
                        slot.spawn((
                            Node {
                                display: Display::None,
                                ..default()
                            },
                            Text::new("✕"),
                            TextFont {
                                font_size: 20.0,
                                ..default()
                            },
                            TextColor(Color::srgb(0.9, 0.1, 0.1)),
                            PlayerHudDead(i),
                        ));
                    });
            }
        });

    // Status text (bottom center)
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(20.0),
                width: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
            StatusText,
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new(""),
                TextFont {
                    font_size: 20.0,
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.9, 0.5)),
            ));
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
            parent.spawn((
                Text::new(""),
                TextFont {
                    font_size: 13.0,
                    ..default()
                },
                TextColor(Color::srgb(0.8, 0.5, 0.5)),
                SidebarEffects,
            ));
        });

    // Player sidebar for player info on hover
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
                TextFont {
                    font_size: 22.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                PlayerSidebarName,
            ));
            parent.spawn((
                Text::new(""),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
                TextColor(Color::srgb(0.8, 0.8, 0.8)),
                PlayerSidebarDescription,
            ));
            parent.spawn((
                Text::new(""),
                TextFont {
                    font_size: 13.0,
                    ..default()
                },
                TextColor(Color::srgb(0.7, 0.8, 0.9)),
                PlayerSidebarStats,
            ));
            parent.spawn((
                Text::new(""),
                TextFont {
                    font_size: 13.0,
                    ..default()
                },
                TextColor(Color::srgb(0.8, 0.5, 0.5)),
                PlayerSidebarEffects,
            ));
            parent.spawn((
                Text::new(""),
                TextFont {
                    font_size: 13.0,
                    ..default()
                },
                TextColor(Color::srgb(0.9, 0.8, 0.3)),
                PlayerSidebarInventory,
            ));
        });

    rendering::spawn_placeholder_room(&mut commands, IVec2::ZERO);
}

pub fn update_room_count(queue: Res<RoomQueue>, mut query: Query<&mut Text, With<RoomCountText>>) {
    let count = queue.len();
    for mut text in &mut query {
        **text = format!("Rooms available: {count}");
    }
}

pub fn update_turn_display(
    turn: Res<TurnNumber>,
    mut query: Query<&mut Text, With<TurnNumberText>>,
) {
    if !turn.is_changed() {
        return;
    }
    for mut text in &mut query {
        **text = format!("Turn: {}", turn.0);
    }
}

pub fn update_move_display(
    players: Option<Res<Players>>,
    mut query: Query<(&mut Text, &PlayerHudMove)>,
) {
    let Some(players) = players else { return };
    if !players.is_changed() {
        return;
    }
    for (mut text, hud) in &mut query {
        if let Some(info) = players.0.get(hud.0) {
            if info.remaining_move > 0 {
                **text = format!("Move: {}", info.remaining_move);
            } else {
                **text = String::new();
            }
        }
    }
}

pub fn update_stats_hud(
    players: Option<Res<Players>>,
    mut query: Query<(&mut Text, &PlayerHudStats)>,
) {
    let Some(players) = players else { return };
    if !players.is_changed() {
        return;
    }
    for (mut text, hud) in &mut query {
        if let Some(info) = players.0.get(hud.0) {
            let s = &info.player.stats;
            **text = format!(
                "STR {} SPD {}\nINT {} SAN {}",
                s.strength, s.speed, s.intelligence, s.sanity
            );
        }
    }
}

pub fn update_end_turn_visibility(
    state: Res<State<GameState>>,
    mut query: Query<&mut Node, With<EndTurnButton>>,
) {
    if !state.is_changed() {
        return;
    }
    let visible = matches!(
        state.get(),
        GameState::SelectingDestinations | GameState::Moving
    );
    for mut node in &mut query {
        node.display = if visible {
            Display::Flex
        } else {
            Display::None
        };
    }
}

pub fn spawn_effect_log_popup(
    mut commands: Commands,
    log_buffer: Res<EffectLogBuffer>,
    players: Option<Res<Players>>,
) {
    let logs = &log_buffer.0;

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
            GlobalZIndex(100),
            EffectLogPopup,
        ))
        .with_children(|overlay| {
            overlay
                .spawn((
                    Node {
                        width: Val::Px(400.0),
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(Val::Px(20.0)),
                        row_gap: Val::Px(12.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.15, 0.15, 0.2)),
                ))
                .with_children(|panel| {
                    panel.spawn((
                        Text::new("Effect Log"),
                        TextFont {
                            font_size: 24.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));

                    // Show source item/event name and description at the top
                    let mut seen_sources = std::collections::HashSet::new();
                    for log in logs {
                        if seen_sources.insert(log.source.clone()) {
                            let kind_label = match log.source_kind {
                                crate::effects::EffectSourceKind::Item => "Item",
                                crate::effects::EffectSourceKind::Event => "Event",
                            };
                            panel.spawn((
                                Text::new(kind_label),
                                TextFont {
                                    font_size: 14.0,
                                    ..default()
                                },
                                TextColor(Color::srgb(0.6, 0.6, 0.6)),
                            ));
                            panel.spawn((
                                Text::new(log.source.clone()),
                                TextFont {
                                    font_size: 18.0,
                                    ..default()
                                },
                                TextColor(Color::srgb(0.9, 0.8, 0.5)),
                            ));
                            if !log.source_description.is_empty() {
                                panel.spawn((
                                    Text::new(log.source_description.clone()),
                                    TextFont {
                                        font_size: 14.0,
                                        ..default()
                                    },
                                    TextColor(Color::srgb(0.7, 0.7, 0.7)),
                                ));
                            }
                        }
                    }

                    // Group effect results by player
                    let mut grouped: std::collections::BTreeMap<
                        usize,
                        Vec<&crate::effects::EffectLog>,
                    > = std::collections::BTreeMap::new();
                    for log in logs {
                        grouped.entry(log.player_idx).or_default().push(log);
                    }

                    for (idx, descs) in &grouped {
                        let name = players
                            .as_ref()
                            .and_then(|p| p.0.get(*idx))
                            .map(|info| info.player.name.as_str())
                            .unwrap_or("Unknown");

                        panel.spawn((
                            Text::new(name.to_string()),
                            TextFont {
                                font_size: 16.0,
                                ..default()
                            },
                            TextColor(PLAYER_COLORS.get(*idx).copied().unwrap_or(Color::WHITE)),
                        ));

                        for log in descs {
                            panel.spawn((
                                Text::new(format!("  - {}", log.description)),
                                TextFont {
                                    font_size: 14.0,
                                    ..default()
                                },
                                TextColor(Color::srgb(0.8, 0.8, 0.8)),
                            ));
                        }
                    }

                    panel
                        .spawn((
                            Button,
                            Node {
                                padding: UiRect::axes(Val::Px(20.0), Val::Px(10.0)),
                                align_self: AlignSelf::Center,
                                margin: UiRect::top(Val::Px(8.0)),
                                ..default()
                            },
                            BackgroundColor(Color::srgb(0.3, 0.5, 0.8)),
                            EffectLogDismissButton,
                        ))
                        .with_child((
                            Text::new("Continue"),
                            TextFont {
                                font_size: 18.0,
                                ..default()
                            },
                            TextColor(Color::WHITE),
                        ));
                });
        });
}

pub fn despawn_effect_log_popup(
    mut commands: Commands,
    popups: Query<Entity, With<EffectLogPopup>>,
    mut log_buffer: ResMut<EffectLogBuffer>,
) {
    for entity in &popups {
        commands.entity(entity).despawn_recursive();
    }
    log_buffer.0.clear();
}

pub fn handle_effect_log_dismiss(
    mut commands: Commands,
    mut next_state: ResMut<NextState<GameState>>,
    resume: Option<Res<ResumeState>>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Query<&Interaction, (Changed<Interaction>, With<EffectLogDismissButton>)>,
) {
    let dismiss = keys.just_pressed(KeyCode::Space)
        || keys.just_pressed(KeyCode::Enter)
        || buttons.iter().any(|i| *i == Interaction::Pressed);

    if !dismiss {
        return;
    }

    if let Some(resume) = resume {
        next_state.set(resume.0.clone());
        commands.remove_resource::<ResumeState>();
    }
}

pub fn handle_reset_view(
    rooms: Query<&rendering::RoomSprite>,
    mut state: ResMut<rendering::CameraState>,
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

fn stat_display_name(stat: &StatName) -> &'static str {
    match stat {
        StatName::Strength => "Strength",
        StatName::Speed => "Speed",
        StatName::Intelligence => "Intelligence",
        StatName::Sanity => "Sanity",
    }
}

fn damage_category_label(stat_a: &StatName) -> &'static str {
    match stat_a {
        StatName::Strength | StatName::Speed => "Physical",
        StatName::Intelligence | StatName::Sanity => "Mental",
    }
}

fn pop_next_allocation(
    commands: &mut Commands,
    queue: &mut DamageAllocationQueue,
    players: &Option<Res<Players>>,
) -> bool {
    loop {
        let Some(pending) = queue.0.pop_front() else {
            return false;
        };

        if players
            .as_ref()
            .and_then(|p| p.0.get(pending.player_idx))
            .is_some_and(|info| info.dead)
        {
            continue;
        }

        let player_name = players
            .as_ref()
            .and_then(|p| p.0.get(pending.player_idx))
            .map(|info| info.player.name.as_str())
            .unwrap_or("Unknown");

        let category = damage_category_label(&pending.stat_a);
        let stat_a_name = stat_display_name(&pending.stat_a);
        let stat_b_name = stat_display_name(&pending.stat_b);
        let total = pending.total;
        let source = pending.source_name.clone();

        let current_a = players
            .as_ref()
            .and_then(|p| p.0.get(pending.player_idx))
            .map(|info| effects::get_stat_value(info, &pending.stat_a))
            .unwrap_or(0);
        let current_b = players
            .as_ref()
            .and_then(|p| p.0.get(pending.player_idx))
            .map(|info| effects::get_stat_value(info, &pending.stat_b))
            .unwrap_or(0);

        let alloc = CurrentAllocation {
            stat_a_value: 0,
            stat_b_value: pending.total,
            pending,
        };

        commands.insert_resource(alloc);
        spawn_damage_popup(commands, category, player_name, total, &source, stat_a_name, stat_b_name, current_a, current_b);
        return true;
    }
}

pub fn enter_allocating_damage(
    mut commands: Commands,
    mut queue: ResMut<DamageAllocationQueue>,
    players: Option<Res<Players>>,
    mut next_state: ResMut<NextState<GameState>>,
    log_buffer: Res<EffectLogBuffer>,
    resume: Option<Res<ResumeState>>,
) {
    if pop_next_allocation(&mut commands, &mut queue, &players) {
        return;
    }

    if !log_buffer.0.is_empty() {
        next_state.set(GameState::ShowingEffectLog);
    } else if let Some(ref resume) = resume {
        next_state.set(resume.0.clone());
        commands.remove_resource::<ResumeState>();
    }
    commands.remove_resource::<DamageAllocationQueue>();
}

fn spawn_damage_popup(
    commands: &mut Commands,
    category: &str,
    player_name: &str,
    total: u32,
    source: &str,
    stat_a_name: &str,
    stat_b_name: &str,
    current_a: i64,
    current_b: i64,
) {

        commands
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
                GlobalZIndex(100),
                DamageAllocationPopup,
            ))
            .with_children(|overlay| {
                overlay
                    .spawn((
                        Node {
                            width: Val::Px(450.0),
                            flex_direction: FlexDirection::Column,
                            padding: UiRect::all(Val::Px(20.0)),
                            row_gap: Val::Px(12.0),
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.15, 0.15, 0.2)),
                    ))
                    .with_children(|panel| {
                        panel.spawn((
                            Text::new(format!("Allocate {category} Damage")),
                            TextFont { font_size: 22.0, ..default() },
                            TextColor(Color::WHITE),
                        ));

                        panel.spawn((
                            Text::new(format!("{player_name} -- {total} damage from {source}")),
                            TextFont { font_size: 14.0, ..default() },
                            TextColor(Color::srgb(0.8, 0.8, 0.8)),
                        ));

                        // Two stat columns side by side
                        panel
                            .spawn(Node {
                                flex_direction: FlexDirection::Row,
                                column_gap: Val::Px(40.0),
                                justify_content: JustifyContent::Center,
                                margin: UiRect::top(Val::Px(8.0)),
                                ..default()
                            })
                            .with_children(|row| {
                                // Stat A column
                                row.spawn(Node {
                                    flex_direction: FlexDirection::Column,
                                    align_items: AlignItems::Center,
                                    row_gap: Val::Px(6.0),
                                    ..default()
                                })
                                .with_children(|col| {
                                    col.spawn((
                                        Text::new(stat_a_name.to_string()),
                                        TextFont { font_size: 16.0, ..default() },
                                        TextColor(Color::srgb(0.9, 0.7, 0.5)),
                                    ));

                                    col.spawn(Node {
                                        flex_direction: FlexDirection::Row,
                                        column_gap: Val::Px(8.0),
                                        align_items: AlignItems::Center,
                                        ..default()
                                    })
                                    .with_children(|btn_row| {
                                        btn_row
                                            .spawn((
                                                Button,
                                                Node {
                                                    padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)),
                                                    ..default()
                                                },
                                                BackgroundColor(Color::srgb(0.4, 0.3, 0.3)),
                                                DamageStatADown,
                                            ))
                                            .with_child((
                                                Text::new("-"),
                                                TextFont { font_size: 20.0, ..default() },
                                                TextColor(Color::WHITE),
                                            ));

                                        btn_row.spawn((
                                            Text::new("0"),
                                            TextFont { font_size: 24.0, ..default() },
                                            TextColor(Color::WHITE),
                                            DamageStatAText,
                                        ));

                                        btn_row
                                            .spawn((
                                                Button,
                                                Node {
                                                    padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)),
                                                    ..default()
                                                },
                                                BackgroundColor(Color::srgb(0.4, 0.3, 0.3)),
                                                DamageStatAUp,
                                            ))
                                            .with_child((
                                                Text::new("+"),
                                                TextFont { font_size: 20.0, ..default() },
                                                TextColor(Color::WHITE),
                                            ));
                                    });

                                    col.spawn((
                                        Text::new(format!("currently: {current_a}")),
                                        TextFont { font_size: 12.0, ..default() },
                                        TextColor(Color::srgb(0.6, 0.6, 0.6)),
                                    ));
                                });

                                // Stat B column
                                row.spawn(Node {
                                    flex_direction: FlexDirection::Column,
                                    align_items: AlignItems::Center,
                                    row_gap: Val::Px(6.0),
                                    ..default()
                                })
                                .with_children(|col| {
                                    col.spawn((
                                        Text::new(stat_b_name.to_string()),
                                        TextFont { font_size: 16.0, ..default() },
                                        TextColor(Color::srgb(0.9, 0.7, 0.5)),
                                    ));

                                    col.spawn(Node {
                                        flex_direction: FlexDirection::Row,
                                        column_gap: Val::Px(8.0),
                                        align_items: AlignItems::Center,
                                        ..default()
                                    })
                                    .with_children(|btn_row| {
                                        btn_row
                                            .spawn((
                                                Button,
                                                Node {
                                                    padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)),
                                                    ..default()
                                                },
                                                BackgroundColor(Color::srgb(0.4, 0.3, 0.3)),
                                                DamageStatBDown,
                                            ))
                                            .with_child((
                                                Text::new("-"),
                                                TextFont { font_size: 20.0, ..default() },
                                                TextColor(Color::WHITE),
                                            ));

                                        btn_row.spawn((
                                            Text::new(format!("{total}")),
                                            TextFont { font_size: 24.0, ..default() },
                                            TextColor(Color::WHITE),
                                            DamageStatBText,
                                        ));

                                        btn_row
                                            .spawn((
                                                Button,
                                                Node {
                                                    padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)),
                                                    ..default()
                                                },
                                                BackgroundColor(Color::srgb(0.4, 0.3, 0.3)),
                                                DamageStatBUp,
                                            ))
                                            .with_child((
                                                Text::new("+"),
                                                TextFont { font_size: 20.0, ..default() },
                                                TextColor(Color::WHITE),
                                            ));
                                    });

                                    col.spawn((
                                        Text::new(format!("currently: {current_b}")),
                                        TextFont { font_size: 12.0, ..default() },
                                        TextColor(Color::srgb(0.6, 0.6, 0.6)),
                                    ));
                                });
                            });

                        panel.spawn((
                            Text::new(format!("{total} / {total}")),
                            TextFont { font_size: 14.0, ..default() },
                            TextColor(Color::srgb(0.7, 0.7, 0.7)),
                            DamageTotalText,
                        ));

                        panel
                            .spawn((
                                Button,
                                Node {
                                    padding: UiRect::axes(Val::Px(20.0), Val::Px(10.0)),
                                    margin: UiRect::top(Val::Px(4.0)),
                                    ..default()
                                },
                                BackgroundColor(Color::srgb(0.3, 0.5, 0.8)),
                                DamageConfirmButton,
                            ))
                            .with_child((
                                Text::new("Confirm"),
                                TextFont { font_size: 18.0, ..default() },
                                TextColor(Color::WHITE),
                            ));
                    });
            });
}

pub fn despawn_damage_allocation_popup(
    mut commands: Commands,
    popups: Query<Entity, With<DamageAllocationPopup>>,
) {
    for entity in &popups {
        commands.entity(entity).despawn_recursive();
    }
    commands.remove_resource::<CurrentAllocation>();
}

pub fn handle_damage_allocation_input(
    alloc: Option<ResMut<CurrentAllocation>>,
    keys: Res<ButtonInput<KeyCode>>,
    a_up: Query<&Interaction, (Changed<Interaction>, With<DamageStatAUp>)>,
    a_down: Query<&Interaction, (Changed<Interaction>, With<DamageStatADown>)>,
    b_up: Query<&Interaction, (Changed<Interaction>, With<DamageStatBUp>)>,
    b_down: Query<&Interaction, (Changed<Interaction>, With<DamageStatBDown>)>,
    mut a_text: Query<&mut Text, (With<DamageStatAText>, Without<DamageStatBText>, Without<DamageTotalText>)>,
    mut b_text: Query<&mut Text, (With<DamageStatBText>, Without<DamageStatAText>, Without<DamageTotalText>)>,
    mut total_text: Query<&mut Text, (With<DamageTotalText>, Without<DamageStatAText>, Without<DamageStatBText>)>,
) {
    let Some(mut alloc) = alloc else { return };
    let total = alloc.pending.total;
    let mut delta: i32 = 0; // positive = move damage toward A

    if keys.just_pressed(KeyCode::ArrowRight) || a_down.iter().any(|i| *i == Interaction::Pressed) || b_up.iter().any(|i| *i == Interaction::Pressed) {
        delta = -1;
    }
    if keys.just_pressed(KeyCode::ArrowLeft) || a_up.iter().any(|i| *i == Interaction::Pressed) || b_down.iter().any(|i| *i == Interaction::Pressed) {
        delta = 1;
    }

    if delta == 0 {
        return;
    }

    let new_a = (alloc.stat_a_value as i32 + delta).clamp(0, total as i32) as u32;
    let new_b = total - new_a;
    alloc.stat_a_value = new_a;
    alloc.stat_b_value = new_b;

    for mut text in &mut a_text {
        **text = format!("{new_a}");
    }
    for mut text in &mut b_text {
        **text = format!("{new_b}");
    }
    for mut text in &mut total_text {
        **text = format!("{} / {total}", new_a + new_b);
    }
}

pub fn handle_damage_confirm(
    mut commands: Commands,
    mut next_state: ResMut<NextState<GameState>>,
    alloc: Option<Res<CurrentAllocation>>,
    mut players: Option<ResMut<Players>>,
    mut log_buffer: ResMut<EffectLogBuffer>,
    mut queue: Option<ResMut<DamageAllocationQueue>>,
    resume: Option<Res<ResumeState>>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Query<&Interaction, (Changed<Interaction>, With<DamageConfirmButton>)>,
    popups: Query<Entity, With<DamageAllocationPopup>>,
) {
    let confirm = keys.just_pressed(KeyCode::Enter)
        || buttons.iter().any(|i| *i == Interaction::Pressed);

    if !confirm {
        return;
    }

    let Some(alloc) = alloc else { return };
    let pending = &alloc.pending;

    if let Some(ref mut players) = players {
        let idx = pending.player_idx;
        if let Some(info) = players.0.get_mut(idx) {
            effects::modify_stat(info, &pending.stat_a, pending.sign * alloc.stat_a_value as i64);
            effects::modify_stat(info, &pending.stat_b, pending.sign * alloc.stat_b_value as i64);
        }

        let stat_a_name = stat_display_name(&pending.stat_a).to_lowercase();
        let stat_b_name = stat_display_name(&pending.stat_b).to_lowercase();
        let category = damage_category_label(&pending.stat_a).to_lowercase();

        log_buffer.0.push(effects::EffectLog {
            player_idx: idx,
            source_kind: pending.source_kind,
            source: pending.source_name.clone(),
            source_description: pending.source_description.clone(),
            description: format!(
                "Took {category} damage ({stat_a_name}:{} {stat_b_name}:{})",
                alloc.stat_a_value, alloc.stat_b_value
            ),
        });

        if effects::check_player_death(&players.0[idx]) {
            players.0[idx].dead = true;
            players.0[idx].remaining_move = 0;
            log_buffer.0.push(effects::EffectLog {
                player_idx: idx,
                source_kind: pending.source_kind,
                source: pending.source_name.clone(),
                source_description: pending.source_description.clone(),
                description: "Died!".to_string(),
            });
        }
    }

    commands.remove_resource::<CurrentAllocation>();

    // Despawn current popup
    for entity in &popups {
        commands.entity(entity).despawn();
    }

    // Try to pop the next allocation and spawn its popup
    if let Some(ref mut queue) = queue {
        let has_next = {
            // We need Option<Res<Players>> but we have Option<ResMut<Players>>
            // Just inline the pop logic here
            loop {
                let Some(pending) = queue.0.front() else {
                    break false;
                };
                let is_dead = players
                    .as_ref()
                    .and_then(|p| p.0.get(pending.player_idx))
                    .is_some_and(|info| info.dead);
                if is_dead {
                    queue.0.pop_front();
                    continue;
                }
                break true;
            }
        };

        if has_next {
            let pending = queue.0.pop_front().unwrap();
            let player_name = players
                .as_ref()
                .and_then(|p| p.0.get(pending.player_idx))
                .map(|info| info.player.name.as_str())
                .unwrap_or("Unknown");
            let category = damage_category_label(&pending.stat_a);
            let stat_a_name = stat_display_name(&pending.stat_a);
            let stat_b_name = stat_display_name(&pending.stat_b);
            let total = pending.total;
            let source = pending.source_name.clone();
            let current_a = players
                .as_ref()
                .and_then(|p| p.0.get(pending.player_idx))
                .map(|info| effects::get_stat_value(info, &pending.stat_a))
                .unwrap_or(0);
            let current_b = players
                .as_ref()
                .and_then(|p| p.0.get(pending.player_idx))
                .map(|info| effects::get_stat_value(info, &pending.stat_b))
                .unwrap_or(0);

            let alloc = CurrentAllocation {
                stat_a_value: 0,
                stat_b_value: pending.total,
                pending,
            };
            commands.insert_resource(alloc);
            spawn_damage_popup(&mut commands, category, player_name, total, &source, stat_a_name, stat_b_name, current_a, current_b);
            return;
        }
    }

    // No more allocations — transition out
    commands.remove_resource::<DamageAllocationQueue>();
    if !log_buffer.0.is_empty() {
        next_state.set(GameState::ShowingEffectLog);
    } else if let Some(ref resume) = resume {
        next_state.set(resume.0.clone());
        commands.remove_resource::<ResumeState>();
    }
}
