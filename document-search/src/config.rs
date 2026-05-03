use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// Path to the turso/SQLite DB file. Resolved relative to the directory
    /// containing `config.toml` if it isn't absolute.
    pub db_path: PathBuf,

    pub ollama: OllamaConfig,

    pub chunking: ChunkingConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OllamaConfig {
    pub url: String,
    pub embedding_model: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChunkingConfig {
    pub chunk_size_bytes: usize,
    pub overlap_bytes: usize,
}

#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error("no config file found; tried: {0:?}")]
    NotFound(Vec<PathBuf>),

    #[error("explicit --config path not found: {0}")]
    ExplicitNotFound(PathBuf),

    #[error("reading {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("parsing {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error(
        "invalid [chunking] config: overlap_bytes ({overlap}) must be less than chunk_size_bytes ({size})"
    )]
    InvalidChunking { size: usize, overlap: usize },

    #[error("invalid [chunking] config: chunk_size_bytes must be > 0")]
    ZeroChunkSize,
}

/// Search order (first existing wins): an explicit `--config` path,
/// `./config.toml`, then `~/.config/document-search/config.toml` (XDG).
pub fn default_search_paths() -> Vec<PathBuf> {
    let mut paths = vec![PathBuf::from("config.toml")];
    if let Some(project) = ProjectDirs::from("", "", "document-search") {
        paths.push(project.config_dir().join("config.toml"));
    }
    paths
}

/// Resolve and load. If `explicit` is `Some`, it must exist or we error
/// rather than silently falling back. Returns the resolved path alongside
/// the parsed config so callers can log it.
pub fn load(explicit: Option<&Path>) -> Result<(PathBuf, Config), ConfigError> {
    let path = match explicit {
        Some(p) => {
            if !p.exists() {
                return Err(ConfigError::ExplicitNotFound(p.to_path_buf()));
            }
            p.to_path_buf()
        }
        None => {
            let candidates = default_search_paths();
            candidates
                .iter()
                .find(|p| p.exists())
                .cloned()
                .ok_or(ConfigError::NotFound(candidates))?
        }
    };

    let raw = std::fs::read_to_string(&path).map_err(|source| ConfigError::Read {
        path: path.clone(),
        source,
    })?;
    let mut cfg: Config = toml::from_str(&raw).map_err(|source| ConfigError::Parse {
        path: path.clone(),
        source,
    })?;
    resolve_relative_db_path(&mut cfg, &path);
    validate_chunking(&cfg.chunking)?;
    Ok((path, cfg))
}

fn validate_chunking(c: &ChunkingConfig) -> Result<(), ConfigError> {
    if c.chunk_size_bytes == 0 {
        return Err(ConfigError::ZeroChunkSize);
    }
    if c.overlap_bytes >= c.chunk_size_bytes {
        return Err(ConfigError::InvalidChunking {
            size: c.chunk_size_bytes,
            overlap: c.overlap_bytes,
        });
    }
    Ok(())
}

fn resolve_relative_db_path(cfg: &mut Config, config_path: &Path) {
    if cfg.db_path.is_absolute() {
        return;
    }
    let base = config_path.parent().unwrap_or_else(|| Path::new("."));
    cfg.db_path = base.join(&cfg.db_path);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn load_resolves_relative_db_path() {
        let dir = tempdir();
        let cfg_path = dir.join("config.toml");
        fs::write(
            &cfg_path,
            r#"
db_path = "data.db"

[ollama]
url = "http://localhost:11434"
embedding_model = "nomic-embed-text"

[chunking]
chunk_size_bytes = 5000
overlap_bytes = 1000
"#,
        )
        .unwrap();

        let (resolved, cfg) = load(Some(&cfg_path)).unwrap();
        assert_eq!(resolved, cfg_path);
        assert_eq!(cfg.db_path, dir.join("data.db"));
    }

    #[test]
    fn load_keeps_absolute_db_path() {
        let dir = tempdir();
        let cfg_path = dir.join("config.toml");
        let abs_db = if cfg!(windows) {
            "C:/tmp/x.db"
        } else {
            "/tmp/x.db"
        };
        fs::write(
            &cfg_path,
            format!(
                r#"
db_path = "{abs_db}"

[ollama]
url = "http://localhost:11434"
embedding_model = "nomic-embed-text"

[chunking]
chunk_size_bytes = 5000
overlap_bytes = 1000
"#
            ),
        )
        .unwrap();

        let (_, cfg) = load(Some(&cfg_path)).unwrap();
        assert_eq!(cfg.db_path, PathBuf::from(abs_db));
    }

    #[test]
    fn explicit_missing_errors() {
        let dir = tempdir();
        let missing = dir.join("nope.toml");
        let err = load(Some(&missing)).unwrap_err();
        assert!(matches!(err, ConfigError::ExplicitNotFound(_)));
    }

    fn tempdir() -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "document-search-cfgtest-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }
}
