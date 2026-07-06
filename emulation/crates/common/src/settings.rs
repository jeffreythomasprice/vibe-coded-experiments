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

/// The discrete speed steps the increase/decrease controls walk through, ordered
/// slowest → fastest. Off-ladder values (e.g. a `3.75×` CLI override) snap to the
/// nearest neighbor in the pressed direction.
const SPEED_LADDER: &[SpeedMode] = &[
    SpeedMode::Relative(0.25),
    SpeedMode::Relative(0.5),
    SpeedMode::Native,
    SpeedMode::Relative(2.0),
    SpeedMode::Relative(3.0),
    SpeedMode::Relative(4.0),
    SpeedMode::Relative(5.0),
    SpeedMode::Unbounded,
];

impl SpeedMode {
    /// This mode's speed as a comparable multiplier (`Unbounded` = infinity), used
    /// to order the ladder and to snap off-ladder values.
    fn factor(self) -> f32 {
        match self {
            SpeedMode::Native => 1.0,
            SpeedMode::Relative(factor) => factor,
            SpeedMode::Unbounded => f32::INFINITY,
        }
    }

    /// The next faster ladder step, or `self` if already at the top (`Unbounded`).
    pub fn increased(self) -> SpeedMode {
        let current = self.factor();
        SPEED_LADDER
            .iter()
            .copied()
            .filter(|mode| mode.factor() > current)
            .min_by(|a, b| a.factor().total_cmp(&b.factor()))
            .unwrap_or(self)
    }

    /// The next slower ladder step, or `self` if already at the bottom.
    pub fn decreased(self) -> SpeedMode {
        let current = self.factor();
        SPEED_LADDER
            .iter()
            .copied()
            .filter(|mode| mode.factor() < current)
            .max_by(|a, b| a.factor().total_cmp(&b.factor()))
            .unwrap_or(self)
    }

    /// A short human label for the speed HUD, without the `Speed: ` prefix:
    /// `"1x"`, `"2x"`, `"3.75x"`, or `"As Fast As Possible"`. `f32`'s `Display`
    /// already renders `2.0` as `"2"`, so no trailing `.0` needs trimming.
    pub fn label(self) -> String {
        match self {
            SpeedMode::Native => "1x".to_string(),
            SpeedMode::Relative(factor) => format!("{factor}x"),
            SpeedMode::Unbounded => "As Fast As Possible".to_string(),
        }
    }

    /// Parse a `--speed` CLI value: `max`/`unbounded`/`inf` (case-insensitive) →
    /// [`SpeedMode::Unbounded`]; otherwise a positive number with an optional
    /// trailing `x` (`2`, `2x`, `3.75`) → [`SpeedMode::Relative`] (or
    /// [`SpeedMode::Native`] for exactly `1`).
    pub fn parse_cli(s: &str) -> Result<SpeedMode, String> {
        let lower = s.trim().to_ascii_lowercase();
        if matches!(lower.as_str(), "max" | "unbounded" | "inf") {
            return Ok(SpeedMode::Unbounded);
        }
        let num = lower.strip_suffix('x').unwrap_or(&lower);
        let factor: f32 = num
            .parse()
            .map_err(|_| format!("invalid speed {s:?}: expected a positive number or \"max\""))?;
        if !(factor > 0.0) {
            return Err(format!("invalid speed {s:?}: must be greater than 0"));
        }
        if factor == 1.0 {
            Ok(SpeedMode::Native)
        } else {
            Ok(SpeedMode::Relative(factor))
        }
    }
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
    fn speed_ladder_steps_and_clamps() {
        assert_eq!(SpeedMode::Native.increased(), SpeedMode::Relative(2.0));
        assert_eq!(SpeedMode::Relative(2.0).decreased(), SpeedMode::Native);
        assert_eq!(SpeedMode::Native.decreased(), SpeedMode::Relative(0.5));
        // Clamp at both ends.
        assert_eq!(
            SpeedMode::Relative(0.25).decreased(),
            SpeedMode::Relative(0.25)
        );
        assert_eq!(SpeedMode::Unbounded.increased(), SpeedMode::Unbounded);
        assert_eq!(SpeedMode::Relative(5.0).increased(), SpeedMode::Unbounded);
        assert_eq!(SpeedMode::Unbounded.decreased(), SpeedMode::Relative(5.0));
    }

    #[test]
    fn off_ladder_value_snaps_to_the_neighbor() {
        assert_eq!(SpeedMode::Relative(3.75).increased(), SpeedMode::Relative(4.0));
        assert_eq!(SpeedMode::Relative(3.75).decreased(), SpeedMode::Relative(3.0));
    }

    #[test]
    fn speed_labels_read_cleanly() {
        assert_eq!(SpeedMode::Native.label(), "1x");
        assert_eq!(SpeedMode::Relative(2.0).label(), "2x");
        assert_eq!(SpeedMode::Relative(0.25).label(), "0.25x");
        assert_eq!(SpeedMode::Relative(3.75).label(), "3.75x");
        assert_eq!(SpeedMode::Unbounded.label(), "As Fast As Possible");
    }

    #[test]
    fn parse_cli_covers_numbers_keywords_and_errors() {
        assert_eq!(SpeedMode::parse_cli("1").unwrap(), SpeedMode::Native);
        assert_eq!(SpeedMode::parse_cli("2x").unwrap(), SpeedMode::Relative(2.0));
        assert_eq!(
            SpeedMode::parse_cli("3.75").unwrap(),
            SpeedMode::Relative(3.75)
        );
        assert_eq!(SpeedMode::parse_cli("MAX").unwrap(), SpeedMode::Unbounded);
        assert_eq!(
            SpeedMode::parse_cli("unbounded").unwrap(),
            SpeedMode::Unbounded
        );
        assert!(SpeedMode::parse_cli("nope").is_err());
        assert!(SpeedMode::parse_cli("0").is_err());
        assert!(SpeedMode::parse_cli("-2").is_err());
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
