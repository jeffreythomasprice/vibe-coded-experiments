use std::path::Path;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::sim::arena::{Arena, ArenaBody, SpawnPoint, boundary_walls};

const MIN_SPAWN_DISTANCE: f32 = 50.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArenaConfig {
    pub width: f32,
    pub height: f32,
    pub bodies: Vec<BodyConfig>,
    pub spawn_points: Vec<SpawnPointConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BodyConfig {
    #[serde(rename = "rectangle")]
    Rectangle {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        #[serde(default)]
        rotation: f32,
    },
    #[serde(rename = "circle")]
    Circle { x: f32, y: f32, radius: f32 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnPointConfig {
    pub x: f32,
    pub y: f32,
    pub facing: f32,
}

impl ArenaConfig {
    pub fn load(path: &Path) -> Result<ArenaConfig> {
        let contents = std::fs::read_to_string(path)?;
        let config: ArenaConfig = toml::from_str(&contents)?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if self.width <= 0.0 || self.height <= 0.0 {
            bail!("arena dimensions must be positive (got {}x{})", self.width, self.height);
        }

        if self.spawn_points.len() < 2 {
            bail!(
                "arena must have at least 2 spawn points (got {})",
                self.spawn_points.len()
            );
        }

        for (i, sp) in self.spawn_points.iter().enumerate() {
            if sp.x < 0.0 || sp.x > self.width || sp.y < 0.0 || sp.y > self.height {
                bail!(
                    "spawn point {} at ({}, {}) is outside arena bounds ({}x{})",
                    i,
                    sp.x,
                    sp.y,
                    self.width,
                    self.height
                );
            }
        }

        for i in 0..self.spawn_points.len() {
            for j in (i + 1)..self.spawn_points.len() {
                let a = &self.spawn_points[i];
                let b = &self.spawn_points[j];
                let dx = a.x - b.x;
                let dy = a.y - b.y;
                let dist = (dx * dx + dy * dy).sqrt();
                if dist < MIN_SPAWN_DISTANCE {
                    bail!(
                        "spawn points {} and {} are too close ({:.1} < {:.1})",
                        i,
                        j,
                        dist,
                        MIN_SPAWN_DISTANCE
                    );
                }
            }
        }

        Ok(())
    }
}

impl From<&ArenaConfig> for Arena {
    fn from(config: &ArenaConfig) -> Self {
        let mut bodies: Vec<ArenaBody> = config
            .bodies
            .iter()
            .map(|b| match b {
                BodyConfig::Rectangle {
                    x,
                    y,
                    width,
                    height,
                    rotation,
                } => ArenaBody::Rectangle {
                    x: *x,
                    y: *y,
                    width: *width,
                    height: *height,
                    rotation: *rotation,
                },
                BodyConfig::Circle { x, y, radius } => ArenaBody::Circle {
                    x: *x,
                    y: *y,
                    radius: *radius,
                },
            })
            .collect();
        bodies.extend(boundary_walls(config.width, config.height));

        let spawn_points = config
            .spawn_points
            .iter()
            .map(|sp| SpawnPoint {
                position: (sp.x, sp.y),
                facing: sp.facing,
            })
            .collect();

        Arena {
            bounds: (config.width, config.height),
            bodies,
            spawn_points,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn valid_config() -> ArenaConfig {
        ArenaConfig {
            width: 800.0,
            height: 600.0,
            bodies: vec![
                BodyConfig::Rectangle {
                    x: 400.0,
                    y: 300.0,
                    width: 100.0,
                    height: 50.0,
                    rotation: 0.0,
                },
                BodyConfig::Circle {
                    x: 200.0,
                    y: 200.0,
                    radius: 30.0,
                },
            ],
            spawn_points: vec![
                SpawnPointConfig {
                    x: 50.0,
                    y: 50.0,
                    facing: 45.0,
                },
                SpawnPointConfig {
                    x: 750.0,
                    y: 550.0,
                    facing: 225.0,
                },
            ],
        }
    }

    #[test]
    fn valid_config_passes_validation() {
        valid_config().validate().unwrap();
    }

    #[test]
    fn spawn_point_out_of_bounds() {
        let mut config = valid_config();
        config.spawn_points[0].x = -10.0;
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("outside arena bounds"));
    }

    #[test]
    fn spawn_point_beyond_width() {
        let mut config = valid_config();
        config.spawn_points[1].x = 900.0;
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("outside arena bounds"));
    }

    #[test]
    fn too_few_spawn_points() {
        let mut config = valid_config();
        config.spawn_points.truncate(1);
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("at least 2 spawn points"));
    }

    #[test]
    fn spawn_points_too_close() {
        let mut config = valid_config();
        config.spawn_points[1].x = 60.0;
        config.spawn_points[1].y = 60.0;
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("too close"));
    }

    #[test]
    fn negative_dimensions() {
        let mut config = valid_config();
        config.width = -100.0;
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("must be positive"));
    }

    #[test]
    fn toml_roundtrip() {
        let config = valid_config();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let deserialized: ArenaConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(deserialized.width, config.width);
        assert_eq!(deserialized.height, config.height);
        assert_eq!(deserialized.bodies.len(), config.bodies.len());
        assert_eq!(deserialized.spawn_points.len(), config.spawn_points.len());
        deserialized.validate().unwrap();
    }

    #[test]
    fn conversion_to_arena() {
        let config = valid_config();
        let arena: Arena = Arena::from(&config);
        assert_eq!(arena.bounds, (800.0, 600.0));
        assert_eq!(arena.bodies.len(), 2 + 4);
        assert_eq!(arena.spawn_points.len(), 2);
        assert!(matches!(arena.bodies[0], ArenaBody::Rectangle { .. }));
        assert!(matches!(arena.bodies[1], ArenaBody::Circle { .. }));
        for body in &arena.bodies[2..] {
            assert!(matches!(body, ArenaBody::Rectangle { .. }));
        }
        assert_eq!(arena.spawn_points[0].position, (50.0, 50.0));
        assert_eq!(arena.spawn_points[0].facing, 45.0);
    }

    #[test]
    fn empty_bodies_config_produces_four_walls() {
        let config = ArenaConfig {
            width: 800.0,
            height: 600.0,
            bodies: vec![],
            spawn_points: vec![
                SpawnPointConfig { x: 50.0, y: 50.0, facing: 0.0 },
                SpawnPointConfig { x: 750.0, y: 550.0, facing: 180.0 },
            ],
        };
        let arena: Arena = Arena::from(&config);
        assert_eq!(arena.bodies.len(), 4);
        for body in &arena.bodies {
            assert!(matches!(body, ArenaBody::Rectangle { .. }));
        }
    }

    #[test]
    fn config_loaded_arena_walls_block_raycast() {
        use crate::config::constants::SimConstants;
        use crate::sim::physics::{build_physics_world, cast_scan_ray};

        let config = ArenaConfig {
            width: 800.0,
            height: 600.0,
            bodies: vec![],
            spawn_points: vec![
                SpawnPointConfig { x: 50.0, y: 50.0, facing: 0.0 },
                SpawnPointConfig { x: 750.0, y: 550.0, facing: 180.0 },
            ],
        };
        let arena: Arena = Arena::from(&config);
        let constants = SimConstants::default();
        let world = build_physics_world(&arena, &constants);

        let result = cast_scan_ray(&world, (400.0, 300.0), 0.0, 1000.0, None);
        assert!(result.is_some(), "ray should hit the right boundary wall");
        let (dist, _) = result.unwrap();
        assert!(dist > 350.0 && dist < 450.0);
    }

    #[test]
    fn load_from_file() {
        let config = valid_config();
        let toml_str = toml::to_string(&config).unwrap();
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(toml_str.as_bytes()).unwrap();
        let loaded = ArenaConfig::load(tmp.path()).unwrap();
        assert_eq!(loaded.width, 800.0);
        assert_eq!(loaded.height, 600.0);
    }
}
