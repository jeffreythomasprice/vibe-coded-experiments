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

use std::time::Duration;

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
    pub saves: SaveStateSettings,
    pub hud: HudSettings,
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

/// Save-state preferences.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct SaveStateSettings {
    /// How many rotating quicksaves to keep per game. Must be at least 1; read
    /// it through [`SaveStateSettings::effective_quicksave_count`].
    pub quicksave_count: u32,
    /// Seconds between periodic autosaves while a game runs; 0 turns them off
    /// (the autosave on exit still happens).
    pub autosave_interval_secs: u64,
}

impl Default for SaveStateSettings {
    fn default() -> SaveStateSettings {
        SaveStateSettings {
            quicksave_count: 5,
            autosave_interval_secs: 300,
        }
    }
}

impl SaveStateSettings {
    /// The time between periodic autosaves, or `None` if they are off.
    pub fn autosave_interval(&self) -> Option<Duration> {
        (self.autosave_interval_secs > 0).then(|| Duration::from_secs(self.autosave_interval_secs))
    }

    /// The configured quicksave count, raised to 1 if it was set to 0.
    pub fn effective_quicksave_count(&self) -> u32 {
        if self.quicksave_count == 0 {
            tracing::warn!("saves.quicksave_count must be at least 1; using 1");
        }
        self.quicksave_count.max(1)
    }
}

/// A corner of the screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Corner {
    #[default]
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// On-screen overlay preferences.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct HudSettings {
    /// The corner the speed box sits in.
    pub speed_corner: Corner,
    /// Seconds after a speed change before the speed box fades. Read it through
    /// [`HudSettings::speed_fade_delay`].
    pub speed_fade_delay_secs: f32,
    /// How see-through the faded speed box is, from 0.0 (opaque) to 1.0
    /// (invisible). Read it through [`HudSettings::speed_faded_opacity`].
    pub speed_faded_transparency: f32,
}

impl Default for HudSettings {
    fn default() -> HudSettings {
        HudSettings {
            speed_corner: Corner::default(),
            speed_fade_delay_secs: 5.0,
            speed_faded_transparency: 0.6,
        }
    }
}

/// The validated form of the speed box's fade settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpeedFade {
    /// How long the box stays opaque after a speed change.
    pub delay: Duration,
    /// The opacity it fades to, in `[0.0, 1.0]`.
    pub faded_opacity: f32,
}

impl SpeedFade {
    /// The opacity the box should have `since_change` after the last speed change.
    pub fn opacity(&self, since_change: Duration) -> f32 {
        if since_change < self.delay {
            1.0
        } else {
            self.faded_opacity
        }
    }
}

impl HudSettings {
    /// The fade settings with out-of-range values corrected (and warned about),
    /// so call this once rather than every frame.
    pub fn speed_fade(&self) -> SpeedFade {
        SpeedFade {
            delay: self.speed_fade_delay(),
            faded_opacity: self.speed_faded_opacity(),
        }
    }

    /// The configured fade delay, or zero if it is negative or not a usable
    /// number of seconds.
    pub fn speed_fade_delay(&self) -> Duration {
        Duration::try_from_secs_f32(self.speed_fade_delay_secs).unwrap_or_else(|err| {
            tracing::warn!(
                value = self.speed_fade_delay_secs,
                %err,
                "invalid hud.speed_fade_delay_secs; using 0"
            );
            Duration::ZERO
        })
    }

    /// The opacity of the faded speed box: one minus the configured
    /// transparency, which is clamped to `[0.0, 1.0]`. A NaN transparency
    /// leaves the box opaque.
    pub fn speed_faded_opacity(&self) -> f32 {
        let transparency = self.speed_faded_transparency;
        if !(0.0..=1.0).contains(&transparency) {
            tracing::warn!(
                value = transparency,
                "hud.speed_faded_transparency must be between 0 and 1; clamping"
            );
        }
        if transparency.is_nan() {
            1.0
        } else {
            1.0 - transparency.clamp(0.0, 1.0)
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
    fn quicksave_count_defaults_to_five_and_is_at_least_one() {
        let parse = |text| toml::from_str::<Settings>(text).unwrap().saves;
        assert_eq!(parse("").effective_quicksave_count(), 5);
        assert_eq!(parse("[saves]").effective_quicksave_count(), 5);
        let nine = parse("[saves]\nquicksave_count = 9");
        assert_eq!(nine.effective_quicksave_count(), 9);
        let zero = parse("[saves]\nquicksave_count = 0");
        assert_eq!(zero.effective_quicksave_count(), 1);
    }

    #[test]
    fn autosave_interval_defaults_to_five_minutes_and_zero_disables_it() {
        let parse = |text| toml::from_str::<Settings>(text).unwrap().saves;
        let five_minutes = Some(Duration::from_secs(300));
        assert_eq!(parse("").autosave_interval(), five_minutes);
        assert_eq!(parse("[saves]").autosave_interval(), five_minutes);
        let short = parse("[saves]\nautosave_interval_secs = 10");
        assert_eq!(short.autosave_interval(), Some(Duration::from_secs(10)));
        let off = parse("[saves]\nautosave_interval_secs = 0");
        assert_eq!(off.autosave_interval(), None);
    }

    #[test]
    fn hud_defaults_to_a_top_left_box_fading_after_five_seconds() {
        let hud = toml::from_str::<Settings>("").unwrap().hud;
        assert_eq!(hud.speed_corner, Corner::TopLeft);
        assert_eq!(hud.speed_fade_delay(), Duration::from_secs(5));
        assert!((hud.speed_faded_opacity() - 0.4).abs() < 1e-6);
    }

    #[test]
    fn partial_hud_section_keeps_other_defaults() {
        let text = "[hud]\nspeed_corner = \"bottom_right\"";
        let hud = toml::from_str::<Settings>(text).unwrap().hud;
        assert_eq!(hud.speed_corner, Corner::BottomRight);
        assert_eq!(hud.speed_fade_delay(), Duration::from_secs(5));
        assert_eq!(hud.speed_faded_transparency, 0.6);
    }

    #[test]
    fn hud_accepts_whole_and_fractional_delays() {
        let delay = |text| toml::from_str::<Settings>(text).unwrap().hud.speed_fade_delay();
        assert_eq!(delay("[hud]\nspeed_fade_delay_secs = 2"), Duration::from_secs(2));
        assert_eq!(
            delay("[hud]\nspeed_fade_delay_secs = 0.5"),
            Duration::from_millis(500)
        );
    }

    #[test]
    fn out_of_range_hud_values_are_clamped() {
        let hud = |delay, transparency| HudSettings {
            speed_fade_delay_secs: delay,
            speed_faded_transparency: transparency,
            ..HudSettings::default()
        };
        assert_eq!(hud(-1.0, 2.0).speed_fade_delay(), Duration::ZERO);
        assert_eq!(hud(f32::NAN, 2.0).speed_fade_delay(), Duration::ZERO);
        assert_eq!(hud(0.0, 2.0).speed_faded_opacity(), 0.0);
        assert_eq!(hud(0.0, -1.0).speed_faded_opacity(), 1.0);
        assert_eq!(hud(0.0, f32::NAN).speed_faded_opacity(), 1.0);
    }

    #[test]
    fn speed_box_fades_once_the_delay_has_passed() {
        let fade = HudSettings::default().speed_fade();
        assert_eq!(fade.opacity(Duration::ZERO), 1.0);
        assert_eq!(fade.opacity(Duration::from_millis(4999)), 1.0);
        assert_eq!(fade.opacity(Duration::from_secs(5)), fade.faded_opacity);
    }

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
        use crate::input::{Action, GenericAction};
        let settings: Settings = toml::from_str("").unwrap();
        assert_eq!(
            settings.input.0.triggers(&GenericAction::Menu),
            GenericAction::Menu.default_triggers()
        );
    }

    #[test]
    fn empty_input_array_is_unbound() {
        use crate::input::GenericAction;
        let settings: Settings = toml::from_str("[input]\nmenu = []\n").unwrap();
        assert!(settings.input.0.triggers(&GenericAction::Menu).is_empty());
    }
}
