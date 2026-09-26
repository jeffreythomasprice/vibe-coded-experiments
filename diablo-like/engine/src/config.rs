use std::path::PathBuf;

use serde::Deserialize;
use thiserror::Error;

pub const DEFAULT_FILTER: &str = "warn,engine=trace,desktop=trace,web=trace";
const FILTER_ENV: &str = "RUST_LOG";
#[cfg(not(target_arch = "wasm32"))]
const FILE_NAME: &str = "config.toml";

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to parse config: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("failed to read config at {}: {source}", path.display())]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to locate the running executable: {0}")]
    ExePath(std::io::Error),
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub logging: Logging,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Logging {
    pub filter: Option<String>,
    pub file: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterOrigin {
    Env,
    File,
    Default,
}

impl Config {
    pub fn parse(text: &str) -> Result<Self, ConfigError> {
        Ok(toml::from_str(text)?)
    }
}

impl Logging {
    pub fn effective_filter(&self) -> (String, FilterOrigin) {
        if let Some(value) = std::env::var(FILTER_ENV)
            .ok()
            .filter(|value| !value.trim().is_empty())
        {
            return (normalize(&value), FilterOrigin::Env);
        }
        match self
            .filter
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Some(value) => (normalize(value), FilterOrigin::File),
            None => (DEFAULT_FILTER.to_string(), FilterOrigin::Default),
        }
    }
}

/// Both `EnvFilter` and `Targets` split on ',' without trimming, so
/// `"warn, engine=trace"` would otherwise parse as a target named " engine"
/// that matches nothing.
fn normalize(filter: &str) -> String {
    filter
        .split(',')
        .map(str::trim)
        .filter(|directive| !directive.is_empty())
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(not(target_arch = "wasm32"))]
pub struct Loaded {
    pub config: Config,
    /// `None` when no config file was found and defaults are in use.
    pub path: Option<PathBuf>,
}

#[cfg(not(target_arch = "wasm32"))]
pub fn load() -> Result<Loaded, ConfigError> {
    if let Some(loaded) = read(PathBuf::from(FILE_NAME))? {
        return Ok(loaded);
    }
    let exe = std::env::current_exe().map_err(ConfigError::ExePath)?;
    if let Some(dir) = exe.parent() {
        if let Some(loaded) = read(dir.join(FILE_NAME))? {
            return Ok(loaded);
        }
    }
    Ok(Loaded {
        config: Config::default(),
        path: None,
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn read(path: PathBuf) -> Result<Option<Loaded>, ConfigError> {
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(ConfigError::Read { path, source: err }),
    };
    Ok(Some(Loaded {
        config: Config::parse(&text)?,
        path: Some(path),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_yields_defaults() {
        let config = Config::parse("").unwrap();
        assert!(config.logging.filter.is_none());
        assert!(config.logging.file.is_none());
    }

    #[test]
    fn unknown_key_is_rejected() {
        assert!(Config::parse("[logging]\nfliter = \"warn\"\n").is_err());
    }

    #[test]
    fn directives_are_trimmed() {
        let config = Config::parse("[logging]\nfilter = \"warn, engine=trace\"\n").unwrap();
        assert_eq!(config.logging.effective_filter().0, "warn,engine=trace");
    }
}
