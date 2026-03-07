use std::time::Duration;

use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;

use crate::arena::CameraState;
use crate::effects::EffectLifetime;
use crate::projectile::ProjectileMarker;
use crate::robot::{do_spawn_robots, LabelMarker, RobotMarker};
use robowar_shared::sim::match_runner::build_match_state;

use crate::keybindings::Keybindings;
use crate::menu::MenuState;
use crate::simulation::{MatchSetup, SimConfig, SimulationState, TickTimer};

#[allow(clippy::too_many_arguments)]
pub fn handle_input(
    keys: Res<ButtonInput<KeyCode>>,
    keybindings: Res<Keybindings>,
    mut sim_config: ResMut<SimConfig>,
    mut tick_timer: ResMut<TickTimer>,
    mut sim_state: ResMut<SimulationState>,
    setup: Res<MatchSetup>,
    mut menu_state: ResMut<MenuState>,
    mut commands: Commands,
    projectile_query: Query<Entity, With<ProjectileMarker>>,
    effect_query: Query<Entity, With<EffectLifetime>>,
    robot_query: Query<Entity, With<RobotMarker>>,
    label_query: Query<Entity, With<LabelMarker>>,
) {
    if keybindings.any_just_pressed(&keys, &keybindings.menu) {
        if menu_state.confirm_quit {
            menu_state.confirm_quit = false;
        } else if menu_state.controls_open {
            menu_state.controls_open = false;
        } else if menu_state.open {
            menu_state.open = false;
            sim_config.paused = menu_state.was_paused;
        } else {
            menu_state.was_paused = sim_config.paused;
            menu_state.open = true;
            sim_config.paused = true;
        }
        return;
    }

    if menu_state.open {
        return;
    }

    if keybindings.any_just_pressed(&keys, &keybindings.pause) {
        sim_config.paused = !sim_config.paused;
        info!("Paused: {}", sim_config.paused);
    }

    if keybindings.any_just_pressed(&keys, &keybindings.speed_up) {
        let new_speed = (sim_config.speed * 2.0).min(16.0);
        if new_speed != sim_config.speed {
            sim_config.speed = new_speed;
            update_timer_period(&sim_config, &mut tick_timer);
            info!("Speed: {}x", sim_config.speed);
        }
    }

    if keybindings.any_just_pressed(&keys, &keybindings.speed_down) {
        let new_speed = (sim_config.speed / 2.0).max(0.25);
        if new_speed != sim_config.speed {
            sim_config.speed = new_speed;
            update_timer_period(&sim_config, &mut tick_timer);
            info!("Speed: {}x", sim_config.speed);
        }
    }

    if keybindings.any_just_pressed(&keys, &keybindings.restart) {
        let (match_state, robot_names, _initial_hp) = build_match_state(&setup.config);
        sim_state.match_state = match_state;
        sim_state.robot_names = robot_names;

        update_timer_period(&sim_config, &mut tick_timer);
        sim_config.paused = false;

        for entity in &projectile_query {
            commands.entity(entity).despawn();
        }
        for entity in &effect_query {
            commands.entity(entity).despawn();
        }

        for entity in &robot_query {
            commands.entity(entity).despawn_recursive();
        }
        for entity in &label_query {
            commands.entity(entity).despawn();
        }

        do_spawn_robots(commands.reborrow(), &sim_state, &sim_config);

        info!("Match restarted");
    }
}

fn update_timer_period(sim_config: &SimConfig, tick_timer: &mut TickTimer) {
    let tick_rate = sim_config.constants.tick_rate as f32;
    let period = 1.0 / (tick_rate * sim_config.speed);
    tick_timer
        .timer
        .set_duration(Duration::from_secs_f32(period));
    tick_timer.timer.reset();
}

#[allow(clippy::too_many_arguments)]
pub fn camera_controls(
    keys: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    keybindings: Res<Keybindings>,
    menu_state: Res<MenuState>,
    mut scroll_events: EventReader<MouseWheel>,
    windows: Query<&Window>,
    time: Res<Time>,
    mut camera_state: ResMut<CameraState>,
    mut camera_query: Query<(&mut Transform, &mut OrthographicProjection), With<Camera2d>>,
) {
    if menu_state.open {
        camera_state.last_cursor_pos = None;
        return;
    }

    let (mut transform, mut projection) = camera_query.single_mut();
    let window = windows.single();

    let window_w = window.resolution.width();
    let window_h = window.resolution.height();
    let arena_w = camera_state.arena_width;
    let arena_h = camera_state.arena_height;
    let padding = camera_state.robot_radius * 4.0;
    let padded_w = arena_w + padding * 2.0;
    let padded_h = arena_h + padding * 2.0;
    let max_scale = (padded_w / window_w).max(padded_h / window_h);
    let min_scale = (camera_state.robot_radius * 4.0) / window_w.min(window_h);

    let dt = time.delta_secs();

    // Keyboard zoom
    if keybindings.any_pressed(&keys, &keybindings.zoom_in) {
        projection.scale *= 1.0 - 2.0 * dt;
    }
    if keybindings.any_pressed(&keys, &keybindings.zoom_out) {
        projection.scale *= 1.0 + 2.0 * dt;
    }

    // Mouse wheel zoom
    for event in scroll_events.read() {
        let scroll = event.y;
        if scroll > 0.0 {
            projection.scale *= 0.9;
        } else if scroll < 0.0 {
            projection.scale *= 1.1;
        }
    }

    projection.scale = projection.scale.clamp(min_scale, max_scale);

    // Keyboard pan
    let pan_speed = camera_state.pan_speed * projection.scale * dt;
    if keybindings.any_pressed(&keys, &keybindings.pan_right) {
        transform.translation.x += pan_speed;
    }
    if keybindings.any_pressed(&keys, &keybindings.pan_left) {
        transform.translation.x -= pan_speed;
    }
    if keybindings.any_pressed(&keys, &keybindings.pan_up) {
        transform.translation.y += pan_speed;
    }
    if keybindings.any_pressed(&keys, &keybindings.pan_down) {
        transform.translation.y -= pan_speed;
    }

    // Mouse drag pan
    if mouse_buttons.pressed(MouseButton::Left) {
        if let Some(cursor_pos) = window.cursor_position() {
            if let Some(last_pos) = camera_state.last_cursor_pos {
                let delta = cursor_pos - last_pos;
                transform.translation.x -= delta.x * projection.scale;
                transform.translation.y += delta.y * projection.scale;
            }
            camera_state.last_cursor_pos = Some(cursor_pos);
        }
    } else {
        camera_state.last_cursor_pos = None;
    }

    // Clamp position to padded arena bounds, centering when the view exceeds the arena
    let half_view_w = projection.scale * window_w / 2.0;
    let half_view_h = projection.scale * window_h / 2.0;

    let center_x = arena_w / 2.0;
    let center_y = arena_h / 2.0;

    if half_view_w >= padded_w / 2.0 {
        transform.translation.x = center_x;
    } else {
        transform.translation.x = transform
            .translation
            .x
            .clamp(-padding + half_view_w, arena_w + padding - half_view_w);
    }

    if half_view_h >= padded_h / 2.0 {
        transform.translation.y = center_y;
    } else {
        transform.translation.y = transform
            .translation
            .y
            .clamp(-padding + half_view_h, arena_h + padding - half_view_h);
    }
}
