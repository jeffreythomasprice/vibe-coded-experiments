//! Config file location, loading, and parsing.
//!
//! The config is searched for in three places, and the **first one that exists
//! wins**:
//!
//! 1. the path passed on the command line via `--config <PATH>`,
//! 2. `./config.toml` in the current working directory,
//! 3. `~/.config/munchkin/config.toml`.
//!
//! If none of those exist, an all-defaults [`Config`] is used.
//!
//! Loading deliberately does **not** log anything: logging needs the config
//! (the log file path lives in it), so we can't have a working logger yet when
//! the config is read. Instead [`Config::load`] returns a [`LoadedConfig`] that
//! records where the config came from and its raw text, and the caller logs
//! that provenance immediately after initialising logging.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

/// The on-disk config, as parsed from TOML. All fields have defaults, so an
/// empty (or missing) config file is valid.
#[derive(Debug, Deserialize)]
pub struct Config {
    /// Log file shared by every running process. Each process appends; each
    /// line is tagged with its pid and mode. Default: `/tmp/munchkin.log`.
    #[serde(default = "default_log_file")]
    pub log_file: PathBuf,

    /// Single-instance lock file used by the engine to guarantee only one
    /// engine process runs at a time. Default: `/tmp/munchkin-engine.lock`.
    #[serde(default = "default_lock_file")]
    pub lock_file: PathBuf,

    /// turso/SQLite-compatible database file used by the engine. Its parent
    /// directory is created on open. Default: `~/.config/munchkin/engine.db`.
    #[serde(default = "default_database_file")]
    pub database_file: PathBuf,

    /// Unix-domain socket the engine listens on and clients (the tui) connect
    /// to. Default: `/tmp/munchkin-engine.sock`.
    #[serde(default = "default_socket_file")]
    pub socket_file: PathBuf,

    /// Connection + model settings for the Ollama server that backs the AI
    /// players and the referee. A missing `[ollama]` table uses all defaults.
    #[serde(default)]
    pub ollama: OllamaConfig,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            log_file: default_log_file(),
            lock_file: default_lock_file(),
            database_file: default_database_file(),
            socket_file: default_socket_file(),
            ollama: OllamaConfig::default(),
        }
    }
}

/// Where to reach Ollama and which models the AI agents should use.
///
/// Player agents and the referee can use different models (the referee may
/// warrant a stronger one), so the model id is configured per role; both default
/// to the same model. Only the engine consumes these — the types live here
/// because all config loading/parsing is centralised in this crate.
#[derive(Debug, Deserialize)]
pub struct OllamaConfig {
    /// Host the Ollama server is reachable on. Default: `localhost`.
    #[serde(default = "default_ollama_host")]
    pub host: String,

    /// Port the Ollama server listens on. Default: `11434` (Ollama's default).
    #[serde(default = "default_ollama_port")]
    pub port: u16,

    /// Model id used by AI **player** agents (limited-information decisions).
    /// Default: `qwen3.5`.
    #[serde(default = "default_player_model")]
    pub player_model: String,

    /// Model id used by the **referee** / rules-lawyer agent (full-information
    /// legality rulings). Default: `qwen3.5`.
    #[serde(default = "default_referee_model")]
    pub referee_model: String,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        OllamaConfig {
            host: default_ollama_host(),
            port: default_ollama_port(),
            player_model: default_player_model(),
            referee_model: default_referee_model(),
        }
    }
}

impl OllamaConfig {
    /// The base URL of the Ollama HTTP API, e.g. `http://localhost:11434`.
    pub fn base_url(&self) -> String {
        format!("http://{}:{}", self.host, self.port)
    }
}

fn default_log_file() -> PathBuf {
    PathBuf::from("/tmp/munchkin.log")
}

fn default_lock_file() -> PathBuf {
    PathBuf::from("/tmp/munchkin-engine.lock")
}

fn default_database_file() -> PathBuf {
    dirs::config_dir()
        .map(|d| d.join("munchkin"))
        .unwrap_or_else(|| PathBuf::from("/tmp/munchkin"))
        .join("engine.db")
}

fn default_socket_file() -> PathBuf {
    PathBuf::from("/tmp/munchkin-engine.sock")
}

fn default_ollama_host() -> String {
    "localhost".to_string()
}

fn default_ollama_port() -> u16 {
    11434
}

fn default_player_model() -> String {
    "qwen3.5".to_string()
}

fn default_referee_model() -> String {
    "qwen3.5".to_string()
}

/// A loaded [`Config`] plus where it came from, so the caller can log the
/// provenance and contents once logging is up.
pub struct LoadedConfig {
    pub config: Config,
    /// The source of the config: either a file path, or [`Source::Defaults`]
    /// when no config file was found.
    pub source: Source,
    /// The raw TOML text that was parsed, or empty when defaults were used.
    pub raw: String,
}

/// Where a [`Config`] was loaded from.
#[derive(Debug)]
pub enum Source {
    /// Read from this file on disk.
    File(PathBuf),
    /// No config file found anywhere; built-in defaults were used.
    Defaults,
}

impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Source::File(path) => write!(f, "{}", path.display()),
            Source::Defaults => write!(f, "<built-in defaults>"),
        }
    }
}

impl Config {
    /// Load the config, honouring the search order documented at the module
    /// level. `cli_path` is the optional `--config` argument.
    pub fn load(cli_path: Option<&Path>) -> Result<LoadedConfig> {
        for path in candidate_paths(cli_path) {
            if path.is_file() {
                let raw = std::fs::read_to_string(&path)
                    .with_context(|| format!("reading config {}", path.display()))?;
                let config: Config = toml::from_str(&raw)
                    .with_context(|| format!("parsing config {}", path.display()))?;
                return Ok(LoadedConfig {
                    config,
                    source: Source::File(path),
                    raw,
                });
            }
        }

        Ok(LoadedConfig {
            config: Config::default(),
            source: Source::Defaults,
            raw: String::new(),
        })
    }
}

/// Build the ordered list of paths to probe for a config file.
fn candidate_paths(cli_path: Option<&Path>) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Some(cli) = cli_path {
        paths.push(cli.to_path_buf());
    }

    paths.push(PathBuf::from("config.toml"));

    if let Some(config_dir) = dirs::config_dir() {
        paths.push(config_dir.join("munchkin").join("config.toml"));
    }

    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_ollama_table_uses_defaults() {
        // A config with no [ollama] table at all → all Ollama defaults.
        let config: Config = toml::from_str("").expect("empty config parses");
        assert_eq!(config.ollama.host, "localhost");
        assert_eq!(config.ollama.port, 11434);
        assert_eq!(config.ollama.player_model, "qwen3.5");
        assert_eq!(config.ollama.referee_model, "qwen3.5");
        assert_eq!(config.ollama.base_url(), "http://localhost:11434");
    }

    #[test]
    fn partial_ollama_table_fills_remaining_defaults() {
        let config: Config = toml::from_str(
            r#"
            [ollama]
            host = "ollama.lan"
            referee_model = "bigbrain"
            "#,
        )
        .expect("partial ollama config parses");
        assert_eq!(config.ollama.host, "ollama.lan");
        assert_eq!(config.ollama.port, 11434); // default
        assert_eq!(config.ollama.player_model, "qwen3.5"); // default
        assert_eq!(config.ollama.referee_model, "bigbrain");
        assert_eq!(config.ollama.base_url(), "http://ollama.lan:11434");
    }
}
