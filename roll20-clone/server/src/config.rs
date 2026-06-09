use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::Deserialize;

/// Runtime configuration, loaded from a TOML file (see `config.example.toml`).
///
/// Fields fall back to the same defaults baked into the example config, so a
/// missing config file behaves identically to running with the example.
#[derive(Debug, Deserialize)]
pub struct Config {
    /// IP address to bind to. Defaults to `127.0.0.1`.
    #[serde(default = "default_host")]
    pub host: IpAddr,
    /// Port to bind to. Defaults to `8001`.
    #[serde(default = "default_port")]
    pub port: u16,
    /// Directory that holds the rotating log files. Defaults to
    /// `/tmp/roll20-clone/log`.
    #[serde(default = "default_log")]
    pub log: PathBuf,
    /// Path to the SQLite database file. Defaults to
    /// `~/.config/roll20-clone/db`.
    #[serde(default = "default_db")]
    pub db: PathBuf,
}

fn default_host() -> IpAddr {
    IpAddr::V4(Ipv4Addr::LOCALHOST)
}

fn default_port() -> u16 {
    8001
}

fn default_log() -> PathBuf {
    PathBuf::from("/tmp/roll20-clone/log")
}

fn default_db() -> PathBuf {
    PathBuf::from("~/.config/roll20-clone/db")
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            log: default_log(),
            db: default_db(),
        }
    }
}

impl Config {
    /// Load configuration, searching (first wins): the optional CLI path,
    /// `$CWD/config.toml`, then `~/.config/roll20-clone/config.toml`. If none
    /// exist, returns the built-in defaults with a `None` source.
    ///
    /// Errors if an explicit CLI path is missing, or if a found file fails to
    /// parse — both indicate user intent that we shouldn't silently ignore.
    pub fn load(cli: Option<PathBuf>) -> anyhow::Result<(Self, Option<PathBuf>)> {
        match find_config(cli)? {
            Some(path) => {
                let text = std::fs::read_to_string(&path)
                    .with_context(|| format!("reading config file {}", path.display()))?;
                let config: Config = toml::from_str(&text)
                    .with_context(|| format!("parsing config file {}", path.display()))?;
                Ok((config, Some(path)))
            }
            None => Ok((Config::default(), None)),
        }
    }

    /// Tilde-expand the `log` and `db` paths in place.
    pub fn expand_paths(&mut self) -> anyhow::Result<()> {
        self.log = expand_tilde(&self.log)?;
        self.db = expand_tilde(&self.db)?;
        Ok(())
    }

    pub fn socket_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }
}

/// Apply the config search order. An explicit CLI path that doesn't exist is an
/// error (the user asked for a specific file); the implicit locations are simply
/// skipped when absent.
fn find_config(cli: Option<PathBuf>) -> anyhow::Result<Option<PathBuf>> {
    if let Some(path) = cli {
        anyhow::ensure!(
            path.exists(),
            "config file not found: {}",
            path.display()
        );
        return Ok(Some(path));
    }

    let cwd = PathBuf::from("config.toml");
    if cwd.exists() {
        return Ok(Some(cwd));
    }

    if let Some(home) = dirs::config_dir() {
        let user = home.join("roll20-clone").join("config.toml");
        if user.exists() {
            return Ok(Some(user));
        }
    }

    Ok(None)
}

/// Expand a leading `~` or `~/` to the user's home directory. Other paths pass
/// through unchanged. A `~` with no resolvable home is a hard error.
fn expand_tilde(path: &Path) -> anyhow::Result<PathBuf> {
    let s = path.to_string_lossy();
    if s == "~" || s.starts_with("~/") {
        let home = dirs::home_dir().context("cannot resolve home directory for `~` in config path")?;
        let rest = s.strip_prefix("~").unwrap().trim_start_matches('/');
        Ok(if rest.is_empty() { home } else { home.join(rest) })
    } else {
        Ok(path.to_path_buf())
    }
}
