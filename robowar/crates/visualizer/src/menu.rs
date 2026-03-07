use bevy::app::AppExit;
use bevy::prelude::*;

use crate::keybindings::Keybindings;

#[derive(Resource, Default)]
pub struct MenuState {
    pub open: bool,
    pub confirm_quit: bool,
    pub controls_open: bool,
    pub was_paused: bool,
}

#[derive(Component)]
pub struct MenuOverlay;

#[derive(Component)]
pub struct ConfirmOverlay;

#[derive(Component)]
pub struct ControlsOverlay;

#[derive(Component)]
pub enum MenuButton {
    Controls,
    Quit,
}

#[derive(Component)]
pub enum ConfirmButton {
    Yes,
    No,
}

#[derive(Component)]
pub enum ControlsButton {
    Back,
}

const BUTTON_NORMAL: Color = Color::srgb(0.25, 0.25, 0.25);
const BUTTON_HOVERED: Color = Color::srgb(0.35, 0.35, 0.35);
const BUTTON_PRESSED: Color = Color::srgb(0.15, 0.15, 0.15);

fn button_node() -> Node {
    Node {
        width: Val::Px(200.0),
        height: Val::Px(50.0),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        ..default()
    }
}

fn button_text(label: &str) -> (Text, TextFont, TextColor) {
    (
        Text::new(label),
        TextFont {
            font_size: 24.0,
            ..default()
        },
        TextColor(Color::WHITE),
    )
}

pub fn setup_menu(mut commands: Commands, keybindings: Res<Keybindings>) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(20.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
            Visibility::Hidden,
            GlobalZIndex(10),
            MenuOverlay,
        ))
        .with_children(|overlay| {
            overlay
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        padding: UiRect::all(Val::Px(20.0)),
                        row_gap: Val::Px(20.0),
                        border: UiRect::all(Val::Px(2.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.15, 0.15, 0.15)),
                    BorderColor(Color::srgb(0.5, 0.5, 0.5)),
                ))
                .with_children(|panel| {
                    panel.spawn((
                        Text::new("MENU"),
                        TextFont {
                            font_size: 48.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                    panel
                        .spawn((
                            Button,
                            button_node(),
                            BackgroundColor(BUTTON_NORMAL),
                            MenuButton::Controls,
                        ))
                        .with_children(|btn| {
                            btn.spawn(button_text("Controls"));
                        });
                    panel
                        .spawn((
                            Button,
                            button_node(),
                            BackgroundColor(BUTTON_NORMAL),
                            MenuButton::Quit,
                        ))
                        .with_children(|btn| {
                            btn.spawn(button_text("Quit"));
                        });
                });
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
                row_gap: Val::Px(20.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
            Visibility::Hidden,
            GlobalZIndex(11),
            ConfirmOverlay,
        ))
        .with_children(|overlay| {
            overlay
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        padding: UiRect::all(Val::Px(20.0)),
                        row_gap: Val::Px(20.0),
                        border: UiRect::all(Val::Px(2.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.15, 0.15, 0.15)),
                    BorderColor(Color::srgb(0.5, 0.5, 0.5)),
                ))
                .with_children(|panel| {
                    panel.spawn((
                        Text::new("Are you sure you want to quit?"),
                        TextFont {
                            font_size: 32.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                    panel
                        .spawn(Node {
                            flex_direction: FlexDirection::Row,
                            column_gap: Val::Px(20.0),
                            ..default()
                        })
                        .with_children(|row| {
                            row.spawn((
                                Button,
                                button_node(),
                                BackgroundColor(BUTTON_NORMAL),
                                ConfirmButton::Yes,
                            ))
                            .with_children(|btn| {
                                btn.spawn(button_text("Yes"));
                            });
                            row.spawn((
                                Button,
                                button_node(),
                                BackgroundColor(BUTTON_NORMAL),
                                ConfirmButton::No,
                            ))
                            .with_children(|btn| {
                                btn.spawn(button_text("No"));
                            });
                        });
                });
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
                row_gap: Val::Px(20.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
            Visibility::Hidden,
            GlobalZIndex(11),
            ControlsOverlay,
        ))
        .with_children(|overlay| {
            overlay
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        padding: UiRect::all(Val::Px(20.0)),
                        row_gap: Val::Px(12.0),
                        border: UiRect::all(Val::Px(2.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.15, 0.15, 0.15)),
                    BorderColor(Color::srgb(0.5, 0.5, 0.5)),
                ))
                .with_children(|panel| {
                    panel.spawn((
                        Text::new("CONTROLS"),
                        TextFont {
                            font_size: 48.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                    for (label, key) in keybindings.all_bindings() {
                        panel.spawn((
                            Text::new(format!("[{}] {}", key, label)),
                            TextFont {
                                font_size: 18.0,
                                ..default()
                            },
                            TextColor(Color::srgba(0.8, 0.8, 0.8, 1.0)),
                        ));
                    }
                    panel
                        .spawn((
                            Button,
                            button_node(),
                            BackgroundColor(BUTTON_NORMAL),
                            ControlsButton::Back,
                        ))
                        .with_children(|btn| {
                            btn.spawn(button_text("Back"));
                        });
                });
        });
}

#[allow(clippy::type_complexity)]
pub fn update_menu_visibility(
    menu_state: Res<MenuState>,
    mut menu_query: Query<
        &mut Visibility,
        (
            With<MenuOverlay>,
            Without<ConfirmOverlay>,
            Without<ControlsOverlay>,
        ),
    >,
    mut confirm_query: Query<
        &mut Visibility,
        (
            With<ConfirmOverlay>,
            Without<MenuOverlay>,
            Without<ControlsOverlay>,
        ),
    >,
    mut controls_query: Query<
        &mut Visibility,
        (
            With<ControlsOverlay>,
            Without<MenuOverlay>,
            Without<ConfirmOverlay>,
        ),
    >,
) {
    for mut vis in menu_query.iter_mut() {
        *vis = if menu_state.open {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
    for mut vis in confirm_query.iter_mut() {
        *vis = if menu_state.confirm_quit {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
    for mut vis in controls_query.iter_mut() {
        *vis = if menu_state.controls_open {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

#[allow(clippy::type_complexity)]
pub fn handle_menu_interaction(
    mut menu_state: ResMut<MenuState>,
    mut exit: EventWriter<AppExit>,
    mut menu_buttons: Query<
        (&Interaction, &mut BackgroundColor, &MenuButton),
        Changed<Interaction>,
    >,
    mut confirm_buttons: Query<
        (&Interaction, &mut BackgroundColor, &ConfirmButton),
        (Changed<Interaction>, Without<MenuButton>),
    >,
    mut controls_buttons: Query<
        (&Interaction, &mut BackgroundColor, &ControlsButton),
        (
            Changed<Interaction>,
            Without<MenuButton>,
            Without<ConfirmButton>,
        ),
    >,
) {
    for (interaction, mut bg, button) in menu_buttons.iter_mut() {
        match interaction {
            Interaction::Pressed => {
                *bg = BackgroundColor(BUTTON_PRESSED);
                match button {
                    MenuButton::Controls => {
                        menu_state.controls_open = true;
                    }
                    MenuButton::Quit => {
                        menu_state.confirm_quit = true;
                    }
                }
            }
            Interaction::Hovered => {
                *bg = BackgroundColor(BUTTON_HOVERED);
            }
            Interaction::None => {
                *bg = BackgroundColor(BUTTON_NORMAL);
            }
        }
    }

    for (interaction, mut bg, button) in confirm_buttons.iter_mut() {
        match interaction {
            Interaction::Pressed => {
                *bg = BackgroundColor(BUTTON_PRESSED);
                match button {
                    ConfirmButton::Yes => {
                        exit.send(AppExit::Success);
                    }
                    ConfirmButton::No => {
                        menu_state.confirm_quit = false;
                    }
                }
            }
            Interaction::Hovered => {
                *bg = BackgroundColor(BUTTON_HOVERED);
            }
            Interaction::None => {
                *bg = BackgroundColor(BUTTON_NORMAL);
            }
        }
    }

    for (interaction, mut bg, button) in controls_buttons.iter_mut() {
        match interaction {
            Interaction::Pressed => {
                *bg = BackgroundColor(BUTTON_PRESSED);
                match button {
                    ControlsButton::Back => {
                        menu_state.controls_open = false;
                    }
                }
            }
            Interaction::Hovered => {
                *bg = BackgroundColor(BUTTON_HOVERED);
            }
            Interaction::None => {
                *bg = BackgroundColor(BUTTON_NORMAL);
            }
        }
    }
}
