//! User configuration file.
//!
//! Lives at the OS-appropriate config dir (e.g. `~/.config/ecs/config.toml`
//! on Linux). On first launch it is created with default values. Relative
//! paths in the file are resolved against the config file's directory, so a
//! `state_file = "./state.toml"` lands next to the config.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

const DEFAULT_STATE_FILE: &str = "./state.toml";
const DEFAULT_LOG_MAX_SIZE_MB: u64 = 10;
const KNOWN_KEYS: &[&str] = &["log_file", "state_file", "log_max_size_mb"];

#[derive(Default, Serialize, Deserialize)]
struct ConfigFile {
    log_file: Option<String>,
    state_file: Option<String>,
    log_max_size_mb: Option<u64>,
}

pub struct Config {
    pub config_dir: PathBuf,
    pub config_file: PathBuf,
    pub log_file: PathBuf,
    pub state_file: PathBuf,
    pub log_max_size_mb: u64,
    unknown_keys: Vec<String>,
}

impl Config {
    /// Load `<config_dir>/ecs/config.toml`. If missing, the directory and a
    /// default file are created. Any I/O or parse error degrades to defaults
    /// (with an `eprintln!` warning, since tracing isn't up yet).
    pub fn load_or_create() -> Self {
        let path = config_file_path();
        let dir = path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));

        if let Err(e) = fs::create_dir_all(&dir) {
            eprintln!(
                "warning: could not create config dir {}: {}",
                dir.display(),
                e
            );
        }

        let (raw_text, write_defaults) = match fs::read_to_string(&path) {
            Ok(text) => (text, false),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (String::new(), true),
            Err(e) => {
                eprintln!(
                    "warning: could not read config file {}: {} (using defaults)",
                    path.display(),
                    e
                );
                return Self::defaults(dir, path);
            }
        };

        if write_defaults {
            let defaults = default_config_text();
            if let Err(e) = fs::write(&path, defaults.as_bytes()) {
                eprintln!(
                    "warning: could not write default config to {}: {}",
                    path.display(),
                    e
                );
            }
            return Self::defaults(dir, path);
        }

        let parsed: ConfigFile = match toml::from_str(&raw_text) {
            Ok(c) => c,
            Err(e) => {
                eprintln!(
                    "warning: could not parse {}: {} (using defaults)",
                    path.display(),
                    e
                );
                return Self::defaults(dir, path);
            }
        };

        let unknown_keys = collect_unknown_keys(&raw_text);

        let log_file = match parsed.log_file.as_deref() {
            Some(s) => resolve(s, &dir),
            None => default_log_file(),
        };
        let state_file = resolve(
            parsed.state_file.as_deref().unwrap_or(DEFAULT_STATE_FILE),
            &dir,
        );
        let log_max_size_mb = parsed.log_max_size_mb.unwrap_or(DEFAULT_LOG_MAX_SIZE_MB);

        Self {
            config_dir: dir,
            config_file: path,
            log_file,
            state_file,
            log_max_size_mb,
            unknown_keys,
        }
    }

    /// Emit `tracing::warn!` for every unrecognized top-level key encountered
    /// during `load_or_create`. Call this from `main` after tracing has been
    /// initialized.
    pub fn warn_unknown_keys(&self) {
        for key in &self.unknown_keys {
            tracing::warn!("unrecognized key `{}` in config.toml — ignoring", key);
        }
    }

    fn defaults(config_dir: PathBuf, config_file: PathBuf) -> Self {
        let state_file = resolve(DEFAULT_STATE_FILE, &config_dir);
        Self {
            config_dir,
            config_file,
            log_file: default_log_file(),
            state_file,
            log_max_size_mb: DEFAULT_LOG_MAX_SIZE_MB,
            unknown_keys: Vec::new(),
        }
    }
}

fn default_log_file() -> PathBuf {
    if let Some(proj) = ProjectDirs::from("", "", "ecs") {
        proj.data_local_dir().join("ecs.log")
    } else {
        PathBuf::from("ecs.log")
    }
}

fn config_file_path() -> PathBuf {
    if let Some(proj) = ProjectDirs::from("", "", "ecs") {
        proj.config_dir().join("config.toml")
    } else {
        PathBuf::from("ecs-config.toml")
    }
}

fn resolve(path_str: &str, base_dir: &Path) -> PathBuf {
    let p = PathBuf::from(path_str);
    if p.is_absolute() { p } else { base_dir.join(p) }
}

fn collect_unknown_keys(text: &str) -> Vec<String> {
    let Ok(table): Result<toml::Table, _> = toml::from_str(text) else {
        return Vec::new();
    };
    let known: HashSet<&str> = KNOWN_KEYS.iter().copied().collect();
    table
        .keys()
        .filter(|k| !known.contains(k.as_str()))
        .cloned()
        .collect()
}

fn default_config_text() -> String {
    format!(
        "# ecs configuration. Relative paths are resolved against this file's directory.\n\
         # log_file defaults to the platform's local data dir, e.g.\n\
         #   Linux:   ~/.local/share/ecs/ecs.log\n\
         #   Windows: %LOCALAPPDATA%\\ecs\\data\\ecs.log\n\
         #   macOS:   ~/Library/Application Support/ecs/ecs.log\n\
         # log_file = \"/path/to/ecs.log\"\n\
         #\n\
         # On startup, if the log file exceeds this size it is rotated to\n\
         # `<log_file>.old` (overwriting any prior .old). Default: {}.\n\
         # log_max_size_mb = {}\n\
         state_file = \"{}\"\n",
        DEFAULT_LOG_MAX_SIZE_MB, DEFAULT_LOG_MAX_SIZE_MB, DEFAULT_STATE_FILE
    )
}
