use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use audio_recorder_schema::{
    SchemaError,
    device_state::{DeviceState, DeviceStateFile},
    from_json_str, to_json_string_pretty,
};

use crate::{atomic, device::DeviceKey};

#[derive(Debug, thiserror::Error)]
pub enum DeviceStateError {
    #[error("failed to read {}: {source}", path.display())]
    Read { path: PathBuf, source: io::Error },
    #[error("failed to write {}: {source}", path.display())]
    Write { path: PathBuf, source: io::Error },
    #[error("invalid device state file {}: {source}", path.display())]
    Invalid { path: PathBuf, source: SchemaError },
    #[error("could not serialize device state: {0}")]
    Serialize(#[from] SchemaError),
}

#[derive(Debug)]
pub struct DeviceStateStore {
    path: PathBuf,
    file: DeviceStateFile,
}

fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl DeviceStateStore {
    pub fn load(path: &Path) -> Result<Self, DeviceStateError> {
        let file = match std::fs::read_to_string(path) {
            Ok(text) => from_json_str(&text)
                .map_err(|source| DeviceStateError::Invalid { path: path.to_path_buf(), source })?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => DeviceStateFile { devices: HashMap::new() },
            Err(source) => return Err(DeviceStateError::Read { path: path.to_path_buf(), source }),
        };
        Ok(Self { path: path.to_path_buf(), file })
    }

    pub fn is_enabled(&self, key: &DeviceKey) -> bool {
        self.file.devices.get(&key.to_string()).map(|d| d.enabled).unwrap_or(true)
    }

    pub fn known(&self) -> Vec<(DeviceKey, String, bool)> {
        self.file
            .devices
            .iter()
            .filter_map(|(k, v)| k.parse().ok().map(|key| (key, v.description.clone(), v.enabled)))
            .collect()
    }

    pub fn set_enabled(&mut self, key: &DeviceKey, description: &str, enabled: bool) -> Result<(), DeviceStateError> {
        self.file.devices.insert(
            key.to_string(),
            DeviceState { enabled, description: description.to_string(), last_seen_unix: now_unix() },
        );
        self.save()
    }

    pub fn mark_seen(&mut self, key: &DeviceKey, description: &str) -> Result<(), DeviceStateError> {
        let enabled = self.is_enabled(key);
        self.set_enabled(key, description, enabled)
    }

    fn save(&self) -> Result<(), DeviceStateError> {
        let text = to_json_string_pretty(&self.file)?;
        atomic::write(&self.path, text.as_bytes())
            .map_err(|source| DeviceStateError::Write { path: self.path.clone(), source })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::Direction;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("audio-recorder-ds-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("nested/devices.json")
    }

    #[test]
    fn defaults_to_enabled_and_persists() {
        let path = tmp("persist");
        let key = DeviceKey::new(Direction::Input, "mic");
        let mut store = DeviceStateStore::load(&path).unwrap();
        assert!(store.is_enabled(&key));
        store.set_enabled(&key, "Mic", false).unwrap();

        let reloaded = DeviceStateStore::load(&path).unwrap();
        assert!(!reloaded.is_enabled(&key));
        assert!(reloaded.is_enabled(&DeviceKey::new(Direction::Output, "mic")));
    }

    #[test]
    fn mark_seen_keeps_disabled() {
        let path = tmp("seen");
        let key = DeviceKey::new(Direction::Output, "spk");
        let mut store = DeviceStateStore::load(&path).unwrap();
        store.set_enabled(&key, "Spk", false).unwrap();
        store.mark_seen(&key, "Speaker").unwrap();
        assert!(!store.is_enabled(&key));
        assert_eq!(store.known()[0].1, "Speaker");
    }

    #[test]
    fn corrupt_file_is_an_error() {
        let path = tmp("corrupt");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"devices": {"x": {"enabled": "yes"}}}"#).unwrap();
        assert!(matches!(DeviceStateStore::load(&path), Err(DeviceStateError::Invalid { .. })));
    }
}
