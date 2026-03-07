use std::path::Path;

use anyhow::{Context, bail, Result};
use serde::{Deserialize, Serialize};

use crate::config::constants::SimConstants;
use crate::sim::robot::Loadout;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RobotConfig {
    pub name: String,
    pub program: String,
    pub loadout: LoadoutConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadoutConfig {
    pub hp: u32,
    pub speed: u32,
    pub armor: u32,
    pub gun_power: u32,
}

impl LoadoutConfig {
    pub fn total_points(&self) -> u32 {
        self.hp + self.speed + self.armor + self.gun_power
    }
}

impl RobotConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read robot config '{}'", path.display()))?;
        let config: RobotConfig = toml::from_str(&contents)
            .with_context(|| format!("failed to parse robot config '{}'", path.display()))?;
        Ok(config)
    }

    pub fn validate(&self, constants: &SimConstants) -> Result<()> {
        let total = self.loadout.total_points();
        if total > constants.point_budget {
            bail!(
                "robot '{}' loadout uses {} points but budget is {}",
                self.name,
                total,
                constants.point_budget
            );
        }
        Ok(())
    }
}

impl From<&LoadoutConfig> for Loadout {
    fn from(config: &LoadoutConfig) -> Self {
        Loadout {
            hp_points: config.hp,
            speed_points: config.speed,
            armor_points: config.armor,
            gun_points: config.gun_power,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config(name: &str, hp: u32, speed: u32, armor: u32, gun_power: u32) -> RobotConfig {
        RobotConfig {
            name: name.to_string(),
            program: format!("{name}.asm"),
            loadout: LoadoutConfig {
                hp,
                speed,
                armor,
                gun_power,
            },
        }
    }

    #[test]
    fn valid_config_within_budget() {
        let constants = SimConstants::default();
        let config = make_config("Spinner", 5, 5, 5, 5);
        assert!(config.validate(&constants).is_ok());
    }

    #[test]
    fn valid_config_under_budget() {
        let constants = SimConstants::default();
        let config = make_config("Light", 2, 2, 2, 2);
        assert!(config.validate(&constants).is_ok());
    }

    #[test]
    fn over_budget_config_rejected() {
        let constants = SimConstants::default();
        let config = make_config("Cheater", 10, 5, 5, 5);
        let err = config.validate(&constants).unwrap_err();
        assert!(err.to_string().contains("25 points"));
        assert!(err.to_string().contains("budget is 20"));
    }

    #[test]
    fn total_points_calculation() {
        let lc = LoadoutConfig {
            hp: 3,
            speed: 4,
            armor: 5,
            gun_power: 6,
        };
        assert_eq!(lc.total_points(), 18);
    }

    #[test]
    fn toml_serialization_roundtrip() {
        let config = make_config("Spinner", 5, 5, 3, 4);
        let toml_str = toml::to_string(&config).expect("serialize");
        let deserialized: RobotConfig = toml::from_str(&toml_str).expect("deserialize");
        assert_eq!(deserialized.name, "Spinner");
        assert_eq!(deserialized.program, "Spinner.asm");
        assert_eq!(deserialized.loadout.hp, 5);
        assert_eq!(deserialized.loadout.speed, 5);
        assert_eq!(deserialized.loadout.armor, 3);
        assert_eq!(deserialized.loadout.gun_power, 4);
    }

    #[test]
    fn toml_deserialization_from_spec() {
        let toml_str = r#"
name = "Spinner"
program = "spinner.asm"

[loadout]
hp = 5
speed = 5
armor = 3
gun_power = 4
"#;
        let config: RobotConfig = toml::from_str(toml_str).expect("deserialize");
        assert_eq!(config.name, "Spinner");
        assert_eq!(config.loadout.total_points(), 17);
        assert!(config.validate(&SimConstants::default()).is_ok());
    }

    #[test]
    fn conversion_to_loadout() {
        let lc = LoadoutConfig {
            hp: 5,
            speed: 5,
            armor: 3,
            gun_power: 4,
        };
        let loadout: Loadout = Loadout::from(&lc);
        assert_eq!(loadout.hp_points, 5);
        assert_eq!(loadout.speed_points, 5);
        assert_eq!(loadout.armor_points, 3);
        assert_eq!(loadout.gun_points, 4);
    }
}
