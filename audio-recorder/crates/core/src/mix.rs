use std::{io, path::{Path, PathBuf}};

use audio_recorder_schema::{
    SchemaError, from_json_str,
    mix::{ChannelGain, MixSettings},
    to_json_string_pretty,
};

use crate::atomic;

pub const MIX_FILE: &str = "mix.json";

#[derive(Debug, thiserror::Error)]
pub enum MixError {
    #[error("failed to read {}: {source}", path.display())]
    Read { path: PathBuf, source: io::Error },
    #[error("failed to write {}: {source}", path.display())]
    Write { path: PathBuf, source: io::Error },
    #[error("invalid mix settings {}: {source}", path.display())]
    Invalid { path: PathBuf, source: SchemaError },
    #[error("could not serialize mix settings: {0}")]
    Serialize(#[from] SchemaError),
}

pub fn default_settings() -> MixSettings {
    MixSettings { version: 1, leveling: false, auto: false, devices: Default::default(), segments: Default::default() }
}

pub fn load(dir: &Path) -> Result<MixSettings, MixError> {
    let path = dir.join(MIX_FILE);
    match std::fs::read_to_string(&path) {
        Ok(text) => from_json_str(&text).map_err(|source| MixError::Invalid { path, source }),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(default_settings()),
        Err(source) => Err(MixError::Read { path, source }),
    }
}

pub fn save(dir: &Path, mix: &MixSettings) -> Result<(), MixError> {
    let path = dir.join(MIX_FILE);
    let text = to_json_string_pretty(mix)?;
    atomic::write(&path, text.as_bytes()).map_err(|source| MixError::Write { path, source })
}

fn effective(entry: Option<&ChannelGain>) -> f32 {
    match entry {
        Some(c) if c.muted => 0.0,
        Some(c) => c.gain as f32,
        None => 1.0,
    }
}

pub fn segment_gain(mix: &MixSettings, device: &str, file: &str, include_device: bool) -> f32 {
    let dev = if include_device { effective(mix.devices.get(device)) } else { 1.0 };
    dev * effective(mix.segments.get(file))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gain(g: f64, muted: bool) -> ChannelGain {
        ChannelGain { gain: g, muted }
    }

    #[test]
    fn missing_file_is_default_and_round_trips() {
        let dir = std::env::temp_dir().join(format!("audio-recorder-mixfile-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(load(&dir).unwrap().devices.is_empty());
        let mut m = default_settings();
        m.devices.insert("input:a".into(), gain(0.5, true));
        save(&dir, &m).unwrap();
        let back = load(&dir).unwrap();
        assert_eq!(back.devices["input:a"].gain, 0.5);
        assert!(back.devices["input:a"].muted);
    }

    #[test]
    fn gains_combine_and_mute_wins() {
        let mut m = default_settings();
        m.devices.insert("input:a".into(), gain(0.5, false));
        m.segments.insert("f".into(), gain(1.5, false));
        assert_eq!(segment_gain(&m, "input:a", "f", true), 0.75);
        assert_eq!(segment_gain(&m, "input:a", "f", false), 1.5);
        assert_eq!(segment_gain(&m, "input:b", "g", true), 1.0);
        m.devices.insert("input:a".into(), gain(0.5, true));
        assert_eq!(segment_gain(&m, "input:a", "f", true), 0.0);
        assert_eq!(segment_gain(&m, "input:a", "f", false), 1.5);
    }
}
