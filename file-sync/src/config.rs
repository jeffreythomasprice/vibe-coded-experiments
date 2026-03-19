use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct Config {
    pub log_dir: Option<PathBuf>,
    pub sync: Vec<SyncEntity>,
}

#[derive(Debug, Deserialize)]
pub struct SyncEntity {
    pub name: String,
    pub paths: Vec<String>,
}

pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    } else if path == "~"
        && let Some(home) = dirs::home_dir() {
            return home;
        }
    PathBuf::from(path)
}

pub fn load_config(config_path: Option<&Path>) -> Result<Config> {
    let path = if let Some(p) = config_path {
        p.to_path_buf()
    } else if let Ok(env_path) = std::env::var("FILE_SYNC_CONFIG") {
        PathBuf::from(env_path)
    } else {
        
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("~/.config"))
            .join("file-sync")
            .join("config.toml")
    };

    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read config file: {}", path.display()))?;

    let config: Config =
        toml::from_str(&content).with_context(|| "Failed to parse config file")?;

    Ok(config)
}
