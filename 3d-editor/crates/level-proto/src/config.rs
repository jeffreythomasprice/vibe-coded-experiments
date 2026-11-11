use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("reading {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("socket path {path} is {len} bytes; unix sockets allow at most {max}")]
    SocketPathTooLong {
        path: PathBuf,
        len: usize,
        max: usize,
    },
    #[error("parsing {path}: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServerConfig {
    pub idle_exit_secs: u64,
    pub log: Option<PathBuf>,
    pub binary: Option<PathBuf>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            idle_exit_secs: 5,
            log: None,
            binary: None,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct McpConfig {
    pub allow_mutate: bool,
    pub allow_destructive: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub socket: Option<PathBuf>,
    pub server: ServerConfig,
    pub mcp: McpConfig,
    #[serde(skip)]
    source: Option<PathBuf>,
}

fn user_tag() -> String {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if let Ok(meta) = std::fs::metadata("/proc/self") {
            return meta.uid().to_string();
        }
    }
    std::env::var("USER").unwrap_or_else(|_| "user".into())
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

impl Config {
    pub fn default_path() -> Option<PathBuf> {
        if let Some(p) = env_path("LEVEL_EDITOR_CONFIG") {
            return Some(p);
        }
        let base = env_path("XDG_CONFIG_HOME")
            .or_else(|| env_path("HOME").map(|h| h.join(".config")))?;
        Some(base.join("level-editor").join("config.toml"))
    }

    /// Loads `$LEVEL_EDITOR_CONFIG`, else the XDG config file. A missing default file means defaults.
    pub fn load() -> Result<Self, ConfigError> {
        match Self::default_path() {
            Some(path) if path.exists() => Self::load_from(&path),
            _ => {
                let config = Self::default();
                config.validate()?;
                Ok(config)
            }
        }
    }

    fn validate(&self) -> Result<(), ConfigError> {
        const MAX: usize = 100;
        let path = self.socket_path();
        let len = path.as_os_str().len();
        if len > MAX {
            return Err(ConfigError::SocketPathTooLong { path, len, max: MAX });
        }
        Ok(())
    }

    pub fn load_from(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        let mut config: Config = toml::from_str(&text).map_err(|source| ConfigError::Parse {
            path: path.to_path_buf(),
            source,
        })?;
        config.source = Some(path.to_path_buf());
        config.validate()?;
        Ok(config)
    }

    pub fn source(&self) -> Option<&Path> {
        self.source.as_deref()
    }

    pub fn socket_path(&self) -> PathBuf {
        if let Some(p) = &self.socket {
            return p.clone();
        }
        match env_path("XDG_RUNTIME_DIR") {
            Some(dir) => dir.join("level-editor.sock"),
            None => PathBuf::from(format!("/tmp/level-editor-{}.sock", user_tag())),
        }
    }

    pub fn lock_path(&self) -> PathBuf {
        let mut p = self.socket_path().into_os_string();
        p.push(".lock");
        PathBuf::from(p)
    }

    pub fn log_path(&self) -> PathBuf {
        if let Some(p) = &self.server.log {
            return p.clone();
        }
        let mut p = self.socket_path().into_os_string();
        p.push(".log");
        PathBuf::from(p)
    }
}
