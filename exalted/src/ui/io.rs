use std::fs;
use std::path::Path;

use crate::character::Character;

#[derive(Debug, thiserror::Error)]
pub enum IoError {
    #[error("could not read {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("could not parse {path} as a Character: {source}")]
    Parse {
        path: String,
        #[source]
        source: toml::de::Error,
    },
    #[error("could not serialize character: {source}")]
    Serialize {
        #[source]
        source: toml::ser::Error,
    },
    #[error("could not write {path}: {source}")]
    Write {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

pub fn load_character_from_path(path: &Path) -> Result<Character, IoError> {
    let text = fs::read_to_string(path).map_err(|source| IoError::Read {
        path: path.display().to_string(),
        source,
    })?;
    toml::from_str(&text).map_err(|source| IoError::Parse {
        path: path.display().to_string(),
        source,
    })
}

pub fn save_character_to_path(character: &Character, path: &Path) -> Result<(), IoError> {
    let text =
        toml::to_string_pretty(character).map_err(|source| IoError::Serialize { source })?;
    fs::write(path, text.as_bytes()).map_err(|source| IoError::Write {
        path: path.display().to_string(),
        source,
    })?;
    Ok(())
}
