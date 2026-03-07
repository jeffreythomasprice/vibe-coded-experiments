use bevy::prelude::*;

use crate::simulation::{SimConfig, SimulationState};

#[derive(Component)]
pub struct RobotMarker {
    pub id: u32,
}

#[derive(Component)]
pub struct TurretMarker {
    pub robot_id: u32,
}

#[derive(Component)]
pub struct LabelMarker {
    pub robot_id: u32,
}

const ROBOT_COLORS: [Color; 4] = [
    Color::srgb(0.9, 0.2, 0.2),
    Color::srgb(0.2, 0.4, 0.9),
    Color::srgb(0.2, 0.8, 0.3),
    Color::srgb(0.9, 0.8, 0.2),
];

pub fn spawn_robots(
    commands: Commands,
    sim_state: Res<SimulationState>,
    sim_config: Res<SimConfig>,
) {
    do_spawn_robots(commands, &sim_state, &sim_config);
}

pub fn do_spawn_robots(
    mut commands: Commands,
    sim_state: &SimulationState,
    sim_config: &SimConfig,
) {
    let radius = sim_config.constants.robot_radius;

    for (i, robot) in sim_state.match_state.robots.iter().enumerate() {
        let color = ROBOT_COLORS[i % ROBOT_COLORS.len()];
        let name = &sim_state.robot_names[i];

        let robot_entity = commands
            .spawn((
                RobotMarker { id: robot.id },
                Sprite {
                    color,
                    custom_size: Some(Vec2::new(radius * 2.0, radius * 2.0)),
                    ..default()
                },
                Transform::from_xyz(robot.position.0, robot.position.1, 10.0)
                    .with_rotation(Quat::from_rotation_z(robot.heading.to_radians())),
            ))
            .id();

        // Turret barrel child
        let turret = commands
            .spawn((
                TurretMarker { robot_id: robot.id },
                Sprite {
                    color: Color::WHITE,
                    custom_size: Some(Vec2::new(radius * 1.5, 3.0)),
                    anchor: bevy::sprite::Anchor::CenterLeft,
                    ..default()
                },
                Transform::from_xyz(0.0, 0.0, 1.0).with_rotation(Quat::from_rotation_z(
                    robot.turret_bearing.to_radians(),
                )),
            ))
            .id();

        // Name label as standalone entity (not a child, so it doesn't inherit rotation)
        commands.spawn((
            LabelMarker { robot_id: robot.id },
            Text2d::new(name.clone()),
            TextFont {
                font_size: 14.0,
                ..default()
            },
            TextColor(Color::WHITE),
            TextLayout::new_with_justify(JustifyText::Center),
            bevy::sprite::Anchor::TopCenter,
            Transform::from_xyz(
                robot.position.0,
                robot.position.1 - (radius + 12.0),
                10.0,
            ),
        ));

        commands.entity(robot_entity).add_children(&[turret]);
    }
}

pub fn sync_robots(
    sim_state: Res<SimulationState>,
    sim_config: Res<SimConfig>,
    mut commands: Commands,
    mut robot_query: Query<(Entity, &RobotMarker, &mut Transform)>,
    mut turret_query: Query<
        (&TurretMarker, &mut Transform),
        (Without<RobotMarker>, Without<LabelMarker>),
    >,
    mut label_query: Query<
        (Entity, &LabelMarker, &mut Transform),
        (Without<RobotMarker>, Without<TurretMarker>),
    >,
) {
    for (entity, marker, mut transform) in &mut robot_query {
        if let Some(robot) = sim_state
            .match_state
            .robots
            .iter()
            .find(|r| r.id == marker.id)
        {
            if !robot.alive {
                commands.entity(entity).despawn_recursive();
                continue;
            }

            transform.translation.x = robot.position.0;
            transform.translation.y = robot.position.1;
            transform.rotation = Quat::from_rotation_z(robot.heading.to_radians());
        }
    }

    for (marker, mut transform) in &mut turret_query {
        if let Some(robot) = sim_state
            .match_state
            .robots
            .iter()
            .find(|r| r.id == marker.robot_id)
        {
            transform.rotation = Quat::from_rotation_z(robot.turret_bearing.to_radians());
        }
    }

    let label_offset = sim_config.constants.robot_radius + 12.0;
    for (entity, marker, mut transform) in &mut label_query {
        if let Some(robot) = sim_state
            .match_state
            .robots
            .iter()
            .find(|r| r.id == marker.robot_id)
        {
            if !robot.alive {
                commands.entity(entity).despawn();
                continue;
            }
            transform.translation.x = robot.position.0;
            transform.translation.y = robot.position.1 - label_offset;
        }
    }
}
