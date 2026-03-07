use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimConstants {
    pub tick_rate: u32,
    pub cycle_budget: u32,
    pub scan_cost: u32,
    pub fire_cost: u32,

    pub arena_width: f32,
    pub arena_height: f32,

    pub robot_radius: f32,
    pub projectile_speed: f32,
    pub projectile_lifetime: u32,
    pub fire_cooldown: u32,
    pub max_ticks: u32,

    pub point_budget: u32,
    pub base_hp: i32,
    pub hp_per_point: i32,
    pub base_speed: f32,
    pub speed_per_point: f32,
    pub base_armor: i32,
    pub base_gun_power: i32,
    pub gun_power_per_point: i32,

    pub wall_damage_factor: f32,
    pub robot_collision_damage_factor: f32,

    pub turret_rotation_speed: f32,
    pub chassis_rotation_speed: f32,

    pub camera_pan_speed: f32,
}

impl Default for SimConstants {
    fn default() -> Self {
        Self {
            tick_rate: 60,
            cycle_budget: 100,
            scan_cost: 5,
            fire_cost: 3,

            arena_width: 800.0,
            arena_height: 600.0,

            robot_radius: 15.0,
            projectile_speed: 10.0,
            projectile_lifetime: 120,
            fire_cooldown: 10,
            max_ticks: 10_000,

            point_budget: 20,
            base_hp: 50,
            hp_per_point: 10,
            base_speed: 2.0,
            speed_per_point: 0.5,
            base_armor: 0,
            base_gun_power: 5,
            gun_power_per_point: 3,

            wall_damage_factor: 0.5,
            robot_collision_damage_factor: 0.3,

            turret_rotation_speed: 30.0,
            chassis_rotation_speed: 3.0,

            camera_pan_speed: 1500.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialization_roundtrip() {
        let original = SimConstants::default();
        let json = serde_json::to_string(&original).expect("serialize");
        let deserialized: SimConstants = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(deserialized.tick_rate, original.tick_rate);
        assert_eq!(deserialized.cycle_budget, original.cycle_budget);
        assert_eq!(deserialized.arena_width, original.arena_width);
        assert_eq!(deserialized.base_hp, original.base_hp);
        assert_eq!(deserialized.max_ticks, original.max_ticks);
    }

    #[test]
    fn arena_larger_than_robot() {
        let c = SimConstants::default();
        assert!(c.arena_width > c.robot_radius * 4.0);
        assert!(c.arena_height > c.robot_radius * 4.0);
    }

    #[test]
    fn cycle_budget_exceeds_action_costs() {
        let c = SimConstants::default();
        assert!(c.cycle_budget > c.scan_cost);
        assert!(c.cycle_budget > c.fire_cost);
    }
}
