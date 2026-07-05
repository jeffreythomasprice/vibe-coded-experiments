//! Backend-agnostic user settings.
//!
//! These are the settings that mean the same thing regardless of how (or where)
//! they are stored: the graphics scale mode and, eventually, input bindings.
//! Anything specific to a particular storage backend — notably *where* on disk
//! ROMs and saves live — is deliberately **not** here; that is the file-system
//! implementation's own configuration (see the `emulator` crate's `FileStore`).
//!
//! Every field carries `#[serde(default)]` so a partial or empty configuration
//! deserializes into sensible defaults rather than failing.

use serde::{Deserialize, Serialize};

use crate::input::ActionBindings;
use crate::scale::ScaleMode;

/// The full set of persisted, backend-agnostic settings.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub graphics: GraphicsSettings,
    pub audio: AudioSettings,
    pub emulation: EmulationSettings,
    pub input: InputBindings,
}

/// How fast the emulated machine runs relative to real hardware. Like
/// [`ScaleMode`], this is both the runtime type the run loop consumes and the
/// value persisted to settings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeedMode {
    /// Match real Game Boy timing (~59.7275 Hz).
    #[default]
    Native,
    /// Run at `factor`× native speed (e.g. `2.0`). If the host can't keep up it
    /// simply runs as fast as it can.
    Relative(f32),
    /// Run as fast as possible, with no pacing at all.
    Unbounded,
}

/// Emulation-timing preferences.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct EmulationSettings {
    /// How fast to run the machine. Defaults to [`SpeedMode::Native`].
    pub speed: SpeedMode,
}

/// Video output preferences.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GraphicsSettings {
    /// How the emulated frame is scaled into the window. Defaults to [`ScaleMode::Fit`].
    pub scale_mode: ScaleMode,
}

/// Audio output preferences. Backend-agnostic like [`GraphicsSettings`]:
/// anything specific to a particular audio library (device selection, buffer
/// sizes) belongs to that backend, not here.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioSettings {
    /// Whether to open an audio output device at all.
    pub enabled: bool,
    /// Master output gain, clamped to `[0.0, 1.0]`.
    pub volume: f32,
}

impl Default for AudioSettings {
    fn default() -> AudioSettings {
        AudioSettings {
            enabled: true,
            volume: 0.5,
        }
    }
}

/// Bindings for the **generic**, cross-emulator actions (see
/// [`crate::input::GenericAction`]). Emulator-specific bindings (e.g. the Game
/// Boy's buttons) are system-specific and, like the file backend's `[paths]`,
/// live in the backend rather than in these system-agnostic settings.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct InputBindings(pub ActionBindings);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_round_trip_through_toml() {
        let settings = Settings::default();
        let text = toml::to_string(&settings).unwrap();
        let back: Settings = toml::from_str(&text).unwrap();
        assert_eq!(back.graphics.scale_mode, settings.graphics.scale_mode);
        assert_eq!(back.audio.enabled, settings.audio.enabled);
        assert_eq!(back.audio.volume, settings.audio.volume);
    }

    #[test]
    fn empty_document_loads_defaults() {
        let settings: Settings = toml::from_str("").unwrap();
        assert_eq!(settings.graphics.scale_mode, ScaleMode::Fit);
        assert!(settings.audio.enabled);
        assert_eq!(settings.audio.volume, 0.5);
    }

    #[test]
    fn partial_audio_section_keeps_other_defaults() {
        let settings: Settings = toml::from_str("[audio]\nvolume = 0.25\n").unwrap();
        assert_eq!(settings.audio.volume, 0.25);
        assert!(settings.audio.enabled, "unspecified audio fields keep their defaults");
    }

    #[test]
    fn partial_document_keeps_other_defaults() {
        let settings: Settings = toml::from_str("[graphics]\nscale_mode = \"stretch\"\n").unwrap();
        assert_eq!(settings.graphics.scale_mode, ScaleMode::Stretch);
    }

    #[test]
    fn speed_mode_round_trips_through_toml() {
        for speed in [
            SpeedMode::Native,
            SpeedMode::Unbounded,
            SpeedMode::Relative(2.0),
            SpeedMode::Relative(3.5),
        ] {
            let mut settings = Settings::default();
            settings.emulation.speed = speed;
            let text = toml::to_string(&settings).unwrap();
            let back: Settings = toml::from_str(&text).unwrap();
            assert_eq!(back.emulation.speed, speed);
        }
    }

    #[test]
    fn default_speed_is_native() {
        let settings: Settings = toml::from_str("").unwrap();
        assert_eq!(settings.emulation.speed, SpeedMode::Native);
    }

    #[test]
    fn absent_input_section_uses_default_bindings() {
        use crate::input::{GenericAction, InputTrigger, Key};
        let settings: Settings = toml::from_str("").unwrap();
        assert_eq!(
            settings.input.0.triggers(&GenericAction::Menu),
            vec![InputTrigger::Key(Key::Escape)]
        );
    }

    #[test]
    fn empty_input_array_is_unbound() {
        use crate::input::GenericAction;
        let settings: Settings = toml::from_str("[input]\nmenu = []\n").unwrap();
        assert!(settings.input.0.triggers(&GenericAction::Menu).is_empty());
    }
}
