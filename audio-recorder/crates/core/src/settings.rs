use std::{
    io,
    path::{Path, PathBuf},
};

use audio_recorder_schema::{SchemaError, from_json_str, settings::Settings, to_json_string_pretty};

use crate::atomic;

pub const DEFAULT_THEME: &str = "system";

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("failed to read {}: {source}", path.display())]
    Read { path: PathBuf, source: io::Error },
    #[error("failed to write {}: {source}", path.display())]
    Write { path: PathBuf, source: io::Error },
    #[error("invalid settings file {}: {source}", path.display())]
    Invalid { path: PathBuf, source: SchemaError },
    #[error("could not serialize settings: {0}")]
    Serialize(#[from] SchemaError),
}

#[derive(Debug)]
pub struct SettingsStore {
    path: PathBuf,
    settings: Settings,
}

fn default_settings() -> Settings {
    Settings { theme: DEFAULT_THEME.to_string() }
}

impl SettingsStore {
    pub fn load(path: &Path) -> Result<Self, SettingsError> {
        let settings = match std::fs::read_to_string(path) {
            Ok(text) => {
                from_json_str(&text).map_err(|source| SettingsError::Invalid { path: path.to_path_buf(), source })?
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => default_settings(),
            Err(source) => return Err(SettingsError::Read { path: path.to_path_buf(), source }),
        };
        Ok(Self { path: path.to_path_buf(), settings })
    }

    pub fn defaults(path: &Path) -> Self {
        Self { path: path.to_path_buf(), settings: default_settings() }
    }

    pub fn get(&self) -> &Settings {
        &self.settings
    }

    pub fn set(&mut self, settings: Settings) -> Result<(), SettingsError> {
        let text = to_json_string_pretty(&settings)?;
        atomic::write(&self.path, text.as_bytes())
            .map_err(|source| SettingsError::Write { path: self.path.clone(), source })?;
        self.settings = settings;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("audio-recorder-settings-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("nested/settings.json")
    }

    #[test]
    fn defaults_then_persists() {
        let path = tmp("persist");
        let mut store = SettingsStore::load(&path).unwrap();
        assert_eq!(store.get().theme, DEFAULT_THEME);
        store.set(Settings { theme: "gruvbox-dark".into() }).unwrap();
        assert_eq!(SettingsStore::load(&path).unwrap().get().theme, "gruvbox-dark");
    }

    #[test]
    fn corrupt_file_is_an_error() {
        let path = tmp("corrupt");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"theme": 3}"#).unwrap();
        assert!(matches!(SettingsStore::load(&path), Err(SettingsError::Invalid { .. })));
    }
}
