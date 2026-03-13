use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use serde::Deserialize;

#[derive(Deserialize)]
struct ConfigFile {
    chunk_storage_dir: Option<String>,
    log_dir: Option<String>,
    #[serde(default)]
    world: Option<WorldConfig>,
    #[serde(default)]
    lighting: Option<LightingConfig>,
}

#[derive(Deserialize)]
struct LightingConfig {
    sun_angle: Option<f32>,
    ambient: Option<f32>,
    sun_intensity: Option<f32>,
    day_duration_secs: Option<f32>,
}

#[derive(Deserialize)]
struct WorldConfig {
    seed: Option<u32>,
}

const DEFAULT_CHUNK_STORAGE_DIR: &str = "/tmp/voxels";

pub struct Config {
    pub chunk_storage_dir: PathBuf,
    pub log_dir: PathBuf,
    pub seed: Option<u32>,
    pub sun_angle: f32,
    pub ambient: f32,
    pub sun_intensity: f32,
    pub day_duration_secs: f32,
}

impl Config {
    pub fn load() -> Result<Config> {
        let config_file = if PathBuf::from("voxels.toml").exists() {
            let contents = fs::read_to_string("voxels.toml")?;
            toml::from_str::<ConfigFile>(&contents)?
        } else {
            ConfigFile {
                chunk_storage_dir: None,
                log_dir: None,
                world: None,
                lighting: None,
            }
        };

        let chunk_storage_dir = PathBuf::from(
            config_file
                .chunk_storage_dir
                .unwrap_or_else(|| DEFAULT_CHUNK_STORAGE_DIR.to_string()),
        );

        let log_dir = PathBuf::from(config_file.log_dir.unwrap_or_else(|| {
            chunk_storage_dir
                .join("logs")
                .to_string_lossy()
                .into_owned()
        }));

        fs::create_dir_all(&chunk_storage_dir)?;
        fs::create_dir_all(&log_dir)?;

        let seed = config_file.world.and_then(|w| w.seed);

        let default_sun_angle = (2.0_f32).atan2(3.0);
        let lighting = config_file.lighting.as_ref();
        let sun_angle = lighting
            .and_then(|l| l.sun_angle)
            .unwrap_or(default_sun_angle);
        let ambient = lighting.and_then(|l| l.ambient).unwrap_or(0.15);
        let sun_intensity = lighting.and_then(|l| l.sun_intensity).unwrap_or(0.85);
        let day_duration_secs = lighting.and_then(|l| l.day_duration_secs).unwrap_or(600.0);

        Ok(Config {
            chunk_storage_dir,
            log_dir,
            seed,
            sun_angle,
            ambient,
            sun_intensity,
            day_duration_secs,
        })
    }
}
