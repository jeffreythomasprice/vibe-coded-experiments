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
            }
        };

        let chunk_storage_dir = PathBuf::from(
            config_file
                .chunk_storage_dir
                .unwrap_or_else(|| DEFAULT_CHUNK_STORAGE_DIR.to_string()),
        );

        let log_dir = PathBuf::from(
            config_file
                .log_dir
                .unwrap_or_else(|| chunk_storage_dir.join("logs").to_string_lossy().into_owned()),
        );

        fs::create_dir_all(&chunk_storage_dir)?;
        fs::create_dir_all(&log_dir)?;

        let seed = config_file.world.and_then(|w| w.seed);

        Ok(Config {
            chunk_storage_dir,
            log_dir,
            seed,
        })
    }
}
