use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

pub const DEFAULT_LOG_FILTER: &str = "warn,diffusion=trace";
pub const DEFAULT_LOG_DIR: &str = "/tmp/diffusion/logs";
pub const DEFAULT_LOG_MAX_BYTES: u64 = 100 * 1024 * 1024;
pub const DEFAULT_LOG_MAX_FILES: usize = 15;
pub const DEFAULT_MODELS_DIR: &str = "/tmp/diffusion";
const FILE_NAME: &str = "config.toml";

#[derive(Debug, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub log_filter: String,
    pub log_dir: PathBuf,
    pub log_max_bytes: u64,
    pub log_max_files: usize,
    pub models_dir: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            log_filter: DEFAULT_LOG_FILTER.to_owned(),
            log_dir: PathBuf::from(DEFAULT_LOG_DIR),
            log_max_bytes: DEFAULT_LOG_MAX_BYTES,
            log_max_files: DEFAULT_LOG_MAX_FILES,
            models_dir: PathBuf::from(DEFAULT_MODELS_DIR),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Loaded {
    pub config: Config,
    pub source: Option<PathBuf>,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("config file not found: {}", .0.display())]
    NotFound(PathBuf),

    #[error("failed to read config file {}: {source}", .path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse config file {}: {source}", .path.display())]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
}

pub fn load(explicit: Option<&Path>) -> Result<Loaded, ConfigError> {
    if let Some(path) = explicit {
        if !path.is_file() {
            return Err(ConfigError::NotFound(path.to_path_buf()));
        }
        return read(path).map(|config| Loaded {
            config,
            source: Some(path.to_path_buf()),
        });
    }

    let mut candidates = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join(FILE_NAME));
    }
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    {
        candidates.push(dir.join(FILE_NAME));
    }
    if let Some(dir) = dirs::config_dir() {
        candidates.push(dir.join("diffusion").join(FILE_NAME));
    }

    load_from_candidates(&candidates)
}

fn load_from_candidates(candidates: &[PathBuf]) -> Result<Loaded, ConfigError> {
    for path in candidates {
        if path.is_file() {
            return read(path).map(|config| Loaded {
                config,
                source: Some(path.clone()),
            });
        }
    }
    Ok(Loaded {
        config: Config::default(),
        source: None,
    })
}

fn read(path: &Path) -> Result<Config, ConfigError> {
    let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    toml::from_str(&text).map_err(|source| ConfigError::Parse {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_existing_candidate_wins() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("first.toml");
        let second = dir.path().join("second.toml");
        std::fs::write(&first, "models_dir = \"/tmp/first\"\n").unwrap();
        std::fs::write(&second, "models_dir = \"/tmp/second\"\n").unwrap();

        let loaded = load_from_candidates(&[first.clone(), second]).unwrap();

        assert_eq!(loaded.source, Some(first));
        assert_eq!(loaded.config.models_dir, PathBuf::from("/tmp/first"));
    }

    #[test]
    fn absent_candidates_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing.toml");
        let present = dir.path().join("present.toml");
        std::fs::write(&present, "models_dir = \"/tmp/present\"\n").unwrap();

        let loaded = load_from_candidates(&[missing, present.clone()]).unwrap();

        assert_eq!(loaded.source, Some(present));
        assert_eq!(loaded.config.models_dir, PathBuf::from("/tmp/present"));
    }

    #[test]
    fn no_candidates_uses_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing.toml");

        let loaded = load_from_candidates(&[missing]).unwrap();

        assert_eq!(loaded.source, None);
        assert_eq!(loaded.config, Config::default());
    }

    #[test]
    fn partial_file_keeps_other_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "log_filter = \"info\"\n").unwrap();

        let loaded = load_from_candidates(&[path]).unwrap();

        assert_eq!(loaded.config.log_filter, "info");
        assert_eq!(loaded.config.models_dir, PathBuf::from(DEFAULT_MODELS_DIR));
    }

    #[test]
    fn log_dir_can_be_overridden() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "log_dir = \"/var/log/diffusion\"\n").unwrap();

        let loaded = load_from_candidates(&[path]).unwrap();

        assert_eq!(loaded.config.log_dir, PathBuf::from("/var/log/diffusion"));
        assert_eq!(loaded.config.models_dir, PathBuf::from(DEFAULT_MODELS_DIR));
    }

    #[test]
    fn unknown_key_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "not_a_real_key = 1\n").unwrap();

        let err = load_from_candidates(std::slice::from_ref(&path)).unwrap_err();

        match err {
            ConfigError::Parse { path: err_path, .. } => assert_eq!(err_path, path),
            other => panic!("expected Parse error, got {other:?}"),
        }
    }

    #[test]
    fn malformed_toml_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "this is not valid toml\n").unwrap();

        let err = load_from_candidates(std::slice::from_ref(&path)).unwrap_err();

        assert!(matches!(err, ConfigError::Parse { .. }));
    }

    #[test]
    fn explicit_missing_path_is_hard_error() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing.toml");

        let err = load(Some(&missing)).unwrap_err();

        assert!(matches!(err, ConfigError::NotFound(p) if p == missing));
    }
}
