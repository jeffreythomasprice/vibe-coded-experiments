use anyhow::{bail, Result};

use crate::config::constants::SimConstants;
use crate::vm::executor::VmState;

#[derive(Debug, Clone)]
pub struct Loadout {
    pub hp_points: u32,
    pub speed_points: u32,
    pub armor_points: u32,
    pub gun_points: u32,
}

impl Loadout {
    pub fn validate(&self, constants: &SimConstants) -> Result<()> {
        let total = self.hp_points + self.speed_points + self.armor_points + self.gun_points;
        if total > constants.point_budget {
            bail!(
                "loadout uses {} points but budget is {}",
                total,
                constants.point_budget
            );
        }
        Ok(())
    }
}

pub struct Robot {
    pub id: u32,
    pub position: (f32, f32),
    pub heading: f32,
    pub turret_bearing: f32,
    pub speed: f32,
    pub hp: i32,
    pub loadout: Loadout,
    pub vm: VmState,
    pub fire_cooldown: u32,
    pub alive: bool,
}

impl Robot {
    pub fn max_hp(&self, constants: &SimConstants) -> i32 {
        constants.base_hp + constants.hp_per_point * self.loadout.hp_points as i32
    }

    pub fn max_speed(&self, constants: &SimConstants) -> f32 {
        constants.base_speed + constants.speed_per_point * self.loadout.speed_points as f32
    }

    pub fn armor(&self, constants: &SimConstants) -> i32 {
        constants.base_armor + self.loadout.armor_points as i32
    }

    pub fn gun_power(&self, constants: &SimConstants) -> i32 {
        constants.base_gun_power + constants.gun_power_per_point * self.loadout.gun_points as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::instruction::Program;

    fn default_constants() -> SimConstants {
        SimConstants::default()
    }

    fn make_loadout(hp: u32, speed: u32, armor: u32, gun: u32) -> Loadout {
        Loadout {
            hp_points: hp,
            speed_points: speed,
            armor_points: armor,
            gun_points: gun,
        }
    }

    fn make_robot(loadout: Loadout) -> Robot {
        Robot {
            id: 0,
            position: (0.0, 0.0),
            heading: 0.0,
            turret_bearing: 0.0,
            speed: 0.0,
            hp: 100,
            loadout,
            vm: VmState::new(Program {
                instructions: vec![],
            }),
            fire_cooldown: 0,
            alive: true,
        }
    }

    #[test]
    fn loadout_validation_within_budget() {
        let c = default_constants();
        let loadout = make_loadout(5, 5, 5, 5);
        assert!(loadout.validate(&c).is_ok());
    }

    #[test]
    fn loadout_validation_exact_budget() {
        let c = default_constants();
        let loadout = make_loadout(5, 5, 5, 5);
        assert_eq!(
            loadout.hp_points + loadout.speed_points + loadout.armor_points + loadout.gun_points,
            c.point_budget
        );
        assert!(loadout.validate(&c).is_ok());
    }

    #[test]
    fn loadout_validation_over_budget() {
        let c = default_constants();
        let loadout = make_loadout(10, 5, 5, 5);
        let err = loadout.validate(&c).unwrap_err();
        assert!(err.to_string().contains("25 points"));
        assert!(err.to_string().contains("budget is 20"));
    }

    #[test]
    fn loadout_validation_zero_points() {
        let c = default_constants();
        let loadout = make_loadout(0, 0, 0, 0);
        assert!(loadout.validate(&c).is_ok());
    }

    #[test]
    fn max_hp_default_constants() {
        let c = default_constants();
        let robot = make_robot(make_loadout(5, 0, 0, 0));
        // base_hp=50, hp_per_point=10, 5 points => 50 + 50 = 100
        assert_eq!(robot.max_hp(&c), 100);
    }

    #[test]
    fn max_hp_zero_points() {
        let c = default_constants();
        let robot = make_robot(make_loadout(0, 0, 0, 0));
        assert_eq!(robot.max_hp(&c), 50);
    }

    #[test]
    fn max_speed_default_constants() {
        let c = default_constants();
        let robot = make_robot(make_loadout(0, 5, 0, 0));
        // base_speed=2.0, speed_per_point=0.5, 5 points => 2.0 + 2.5 = 4.5
        assert_eq!(robot.max_speed(&c), 4.5);
    }

    #[test]
    fn max_speed_zero_points() {
        let c = default_constants();
        let robot = make_robot(make_loadout(0, 0, 0, 0));
        assert_eq!(robot.max_speed(&c), 2.0);
    }

    #[test]
    fn armor_default_constants() {
        let c = default_constants();
        let robot = make_robot(make_loadout(0, 0, 3, 0));
        // base_armor=0, armor_points=3 => 0 + 3 = 3
        assert_eq!(robot.armor(&c), 3);
    }

    #[test]
    fn armor_zero_points() {
        let c = default_constants();
        let robot = make_robot(make_loadout(0, 0, 0, 0));
        assert_eq!(robot.armor(&c), 0);
    }

    #[test]
    fn gun_power_default_constants() {
        let c = default_constants();
        let robot = make_robot(make_loadout(0, 0, 0, 4));
        // base_gun_power=5, gun_power_per_point=3, 4 points => 5 + 12 = 17
        assert_eq!(robot.gun_power(&c), 17);
    }

    #[test]
    fn gun_power_zero_points() {
        let c = default_constants();
        let robot = make_robot(make_loadout(0, 0, 0, 0));
        assert_eq!(robot.gun_power(&c), 5);
    }

    #[test]
    fn balanced_loadout_stats() {
        let c = default_constants();
        let robot = make_robot(make_loadout(5, 5, 5, 5));
        assert_eq!(robot.max_hp(&c), 100);
        assert_eq!(robot.max_speed(&c), 4.5);
        assert_eq!(robot.armor(&c), 5);
        assert_eq!(robot.gun_power(&c), 20);
    }
}
