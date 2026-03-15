use bevy::prelude::*;

use crate::rendering;
use crate::room_queue::RoomQueue;
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
                TextFont { font_size: 28.0, ..default() },
                TurnNumberText,
            ));

            parent.spawn((
                Text::new("Rooms available: 0"),
                TextFont { font_size: 28.0, ..default() },
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
                    TextFont { font_size: 22.0, ..default() },
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
                    TextFont { font_size: 22.0, ..default() },
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
                            TextFont { font_size: 12.0, ..default() },
                            TextColor(Color::WHITE),
                            PlayerHudName(i),
                        ));
                        slot.spawn((
                            Text::new(""),
                            TextFont { font_size: 10.0, ..default() },
                            TextColor(Color::srgb(0.7, 0.9, 0.7)),
                            PlayerHudMove(i),
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
                TextFont { font_size: 20.0, ..default() },
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
                TextFont { font_size: 22.0, ..default() },
                TextColor(Color::WHITE),
                SidebarTitle,
            ));
            parent.spawn((
                Text::new(""),
                TextFont { font_size: 14.0, ..default() },
                TextColor(Color::srgb(0.8, 0.8, 0.8)),
                SidebarDescription,
            ));
            parent.spawn((
                Text::new(""),
                TextFont { font_size: 14.0, ..default() },
                TextColor(Color::srgb(0.9, 0.85, 0.5)),
                SidebarPlayers,
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

    rendering::spawn_placeholder_room(&mut commands, IVec2::ZERO);
}

pub fn update_room_count(queue: Res<RoomQueue>, mut query: Query<&mut Text, With<RoomCountText>>) {
    let count = queue.len();
    for mut text in &mut query {
        **text = format!("Rooms available: {count}");
    }
}

pub fn update_turn_display(turn: Res<TurnNumber>, mut query: Query<&mut Text, With<TurnNumberText>>) {
    if !turn.is_changed() { return; }
    for mut text in &mut query {
        **text = format!("Turn: {}", turn.0);
    }
}

pub fn update_move_display(
    players: Option<Res<Players>>,
    mut query: Query<(&mut Text, &PlayerHudMove)>,
) {
    let Some(players) = players else { return };
    if !players.is_changed() { return; }
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

pub fn update_end_turn_visibility(
    state: Res<State<GameState>>,
    mut query: Query<&mut Node, With<EndTurnButton>>,
) {
    if !state.is_changed() { return; }
    let visible = matches!(state.get(), GameState::SelectingDestinations | GameState::Moving);
    for mut node in &mut query {
        node.display = if visible { Display::Flex } else { Display::None };
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
