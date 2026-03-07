use bevy::prelude::*;

use crate::menu::MenuState;
use crate::simulation::{SimConfig, SimulationState};

const ROBOT_COLORS: [Color; 4] = [
    Color::srgb(0.9, 0.2, 0.2),
    Color::srgb(0.2, 0.4, 0.9),
    Color::srgb(0.2, 0.8, 0.3),
    Color::srgb(0.9, 0.8, 0.2),
];

#[derive(Component)]
pub(crate) struct TickText;

#[derive(Component)]
pub(crate) struct StatusText;

#[derive(Component)]
pub(crate) struct HpBarFill {
    robot_id: u32,
}

#[derive(Component)]
pub(crate) struct HpBarLabel {
    robot_id: u32,
}

#[derive(Component)]
pub(crate) struct SpeedText;

#[derive(Component)]
pub(crate) struct PauseOverlay;

fn hp_color(fraction: f32) -> Color {
    if fraction > 0.5 {
        let t = (fraction - 0.5) * 2.0;
        Color::srgb(1.0 - t, 0.8 + 0.2 * t, 0.0)
    } else {
        let t = fraction * 2.0;
        Color::srgb(0.9, t * 0.8, 0.0)
    }
}

pub fn setup_hud(mut commands: Commands, sim_state: Res<SimulationState>) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(0.0),
                left: Val::Px(0.0),
                width: Val::Px(260.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(8.0)),
                row_gap: Val::Px(6.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    width: Val::Percent(100.0),
                    ..default()
                },
            ))
            .with_children(|row| {
                row.spawn((
                    Text::new("Tick: 0"),
                    TextFont {
                        font_size: 16.0,
                        ..default()
                    },
                    TextColor(Color::WHITE),
                    TickText,
                ));
                row.spawn((
                    Text::new("Running"),
                    TextFont {
                        font_size: 16.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.5, 1.0, 0.5)),
                    StatusText,
                ));
            });

            root.spawn((
                Text::new("Speed: 1x"),
                TextFont {
                    font_size: 16.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                SpeedText,
            ));

            for (i, name) in sim_state.robot_names.iter().enumerate() {
                let color = ROBOT_COLORS[i % ROBOT_COLORS.len()];
                let id = i as u32;

                root.spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        width: Val::Percent(100.0),
                        row_gap: Val::Px(2.0),
                        ..default()
                    },
                ))
                .with_children(|col| {
                    col.spawn((
                        Node {
                            flex_direction: FlexDirection::Row,
                            justify_content: JustifyContent::SpaceBetween,
                            width: Val::Percent(100.0),
                            ..default()
                        },
                    ))
                    .with_children(|label_row| {
                        label_row.spawn((
                            Text::new(name.clone()),
                            TextFont {
                                font_size: 14.0,
                                ..default()
                            },
                            TextColor(color),
                        ));
                        label_row.spawn((
                            Text::new("HP"),
                            TextFont {
                                font_size: 14.0,
                                ..default()
                            },
                            TextColor(Color::WHITE),
                            HpBarLabel { robot_id: id },
                        ));
                    });

                    col.spawn((
                        Node {
                            width: Val::Percent(100.0),
                            height: Val::Px(10.0),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.2, 0.2, 0.2, 1.0)),
                    ))
                    .with_children(|bar_bg| {
                        bar_bg.spawn((
                            Node {
                                width: Val::Percent(100.0),
                                height: Val::Percent(100.0),
                                ..default()
                            },
                            BackgroundColor(hp_color(1.0)),
                            HpBarFill { robot_id: id },
                        ));
                    });
                });
            }
        });

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(8.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
            Visibility::Hidden,
            PauseOverlay,
        ))
        .with_children(|overlay| {
            overlay
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        padding: UiRect::all(Val::Px(20.0)),
                        row_gap: Val::Px(8.0),
                        border: UiRect::all(Val::Px(2.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.15, 0.15, 0.15)),
                    BorderColor(Color::srgb(0.5, 0.5, 0.5)),
                ))
                .with_children(|panel| {
                    panel.spawn((
                        Text::new("PAUSED"),
                        TextFont {
                            font_size: 48.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                });
        });
}

fn format_speed(speed: f32) -> String {
    if speed.fract() == 0.0 {
        format!("Speed: {}x", speed as i32)
    } else {
        format!("Speed: {}x", speed)
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn update_hud(
    sim_state: Res<SimulationState>,
    sim_config: Res<SimConfig>,
    mut tick_query: Query<&mut Text, With<TickText>>,
    mut status_query: Query<(&mut Text, &mut TextColor), (With<StatusText>, Without<TickText>)>,
    mut bar_query: Query<
        (&HpBarFill, &mut Node, &mut BackgroundColor),
        (Without<StatusText>, Without<SpeedText>),
    >,
    mut label_query: Query<
        (&HpBarLabel, &mut Text),
        (Without<TickText>, Without<StatusText>, Without<SpeedText>),
    >,
    menu_state: Res<MenuState>,
    mut overlay_query: Query<&mut Visibility, With<PauseOverlay>>,
    mut speed_query: Query<&mut Text, (With<SpeedText>, Without<TickText>, Without<StatusText>)>,
) {
    let ms = &sim_state.match_state;

    for mut text in tick_query.iter_mut() {
        **text = format!("Tick: {}", ms.tick_number);
    }

    for (mut text, mut color) in status_query.iter_mut() {
        if ms.finished {
            if let Some(winner_id) = ms.winner {
                let winner_name = sim_state
                    .robot_names
                    .get(winner_id as usize)
                    .cloned()
                    .unwrap_or_else(|| format!("Robot {}", winner_id));
                **text = format!("{} wins!", winner_name);
            } else {
                **text = "Draw!".to_string();
            }
            *color = TextColor(Color::srgb(1.0, 0.85, 0.0));
        } else if sim_config.paused {
            **text = "Paused".to_string();
            *color = TextColor(Color::srgb(1.0, 1.0, 0.5));
        } else {
            **text = "Running".to_string();
            *color = TextColor(Color::srgb(0.5, 1.0, 0.5));
        }
    }

    for (fill, mut node, mut bg) in bar_query.iter_mut() {
        if let Some(robot) = ms.robots.iter().find(|r| r.id == fill.robot_id) {
            let max_hp = robot.max_hp(&sim_config.constants);
            let fraction = if max_hp > 0 {
                (robot.hp.max(0) as f32) / (max_hp as f32)
            } else {
                0.0
            };
            node.width = Val::Percent(fraction * 100.0);
            *bg = BackgroundColor(hp_color(fraction));
        }
    }

    for (label, mut text) in label_query.iter_mut() {
        if let Some(robot) = ms.robots.iter().find(|r| r.id == label.robot_id) {
            let max_hp = robot.max_hp(&sim_config.constants);
            **text = format!("{}/{}", robot.hp.max(0), max_hp);
        }
    }

    for mut text in speed_query.iter_mut() {
        **text = format_speed(sim_config.speed);
    }

    for mut visibility in overlay_query.iter_mut() {
        *visibility = if sim_config.paused && !ms.finished && !menu_state.open {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}
