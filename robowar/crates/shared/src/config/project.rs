use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Deserialize;

fn string_or_vec<'de, D>(deserializer: D) -> std::result::Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de;

    struct StringOrVec;

    impl<'de> de::Visitor<'de> for StringOrVec {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a string or array of strings")
        }

        fn visit_str<E: de::Error>(self, value: &str) -> std::result::Result<Vec<String>, E> {
            Ok(vec![value.to_string()])
        }

        fn visit_seq<A: de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> std::result::Result<Vec<String>, A::Error> {
            let mut v = Vec::new();
            while let Some(s) = seq.next_element()? {
                v.push(s);
            }
            Ok(v)
        }
    }

    deserializer.deserialize_any(StringOrVec)
}

fn default_pause() -> Vec<String> {
    vec!["Space".to_string()]
}
fn default_speed_up() -> Vec<String> {
    vec!["Equal".to_string()]
}
fn default_speed_down() -> Vec<String> {
    vec!["Minus".to_string()]
}
fn default_restart() -> Vec<String> {
    vec!["R".to_string()]
}
fn default_menu() -> Vec<String> {
    vec!["Escape".to_string()]
}
fn default_zoom_in() -> Vec<String> {
    vec!["BracketRight".to_string()]
}
fn default_zoom_out() -> Vec<String> {
    vec!["BracketLeft".to_string()]
}
fn default_pan_up() -> Vec<String> {
    vec!["W".to_string(), "ArrowUp".to_string()]
}
fn default_pan_down() -> Vec<String> {
    vec!["S".to_string(), "ArrowDown".to_string()]
}
fn default_pan_left() -> Vec<String> {
    vec!["A".to_string(), "ArrowLeft".to_string()]
}
fn default_pan_right() -> Vec<String> {
    vec!["D".to_string(), "ArrowRight".to_string()]
}

#[derive(Debug, Deserialize)]
pub struct KeybindingsConfig {
    #[serde(default = "default_pause", deserialize_with = "string_or_vec")]
    pub pause: Vec<String>,
    #[serde(default = "default_speed_up", deserialize_with = "string_or_vec")]
    pub speed_up: Vec<String>,
    #[serde(default = "default_speed_down", deserialize_with = "string_or_vec")]
    pub speed_down: Vec<String>,
    #[serde(default = "default_restart", deserialize_with = "string_or_vec")]
    pub restart: Vec<String>,
    #[serde(default = "default_menu", deserialize_with = "string_or_vec")]
    pub menu: Vec<String>,
    #[serde(default = "default_zoom_in", deserialize_with = "string_or_vec")]
    pub zoom_in: Vec<String>,
    #[serde(default = "default_zoom_out", deserialize_with = "string_or_vec")]
    pub zoom_out: Vec<String>,
    #[serde(default = "default_pan_up", deserialize_with = "string_or_vec")]
    pub pan_up: Vec<String>,
    #[serde(default = "default_pan_down", deserialize_with = "string_or_vec")]
    pub pan_down: Vec<String>,
    #[serde(default = "default_pan_left", deserialize_with = "string_or_vec")]
    pub pan_left: Vec<String>,
    #[serde(default = "default_pan_right", deserialize_with = "string_or_vec")]
    pub pan_right: Vec<String>,
}

impl Default for KeybindingsConfig {
    fn default() -> Self {
        Self {
            pause: default_pause(),
            speed_up: default_speed_up(),
            speed_down: default_speed_down(),
            restart: default_restart(),
            menu: default_menu(),
            zoom_in: default_zoom_in(),
            zoom_out: default_zoom_out(),
            pan_up: default_pan_up(),
            pan_down: default_pan_down(),
            pan_left: default_pan_left(),
            pan_right: default_pan_right(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct VisualizerConfig {
    pub speed: Option<f32>,
    #[serde(default)]
    pub keybindings: KeybindingsConfig,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
pub struct LoggingConfig {
    pub log_dir: Option<PathBuf>,
    #[serde(default = "default_true")]
    pub file_logging: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            log_dir: None,
            file_logging: true,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct ProjectConfig {
    pub arena: Option<PathBuf>,
    pub robots: Option<Vec<PathBuf>>,
    pub max_ticks: Option<u32>,
    pub seed: Option<u64>,
    pub groups: Option<HashMap<String, Vec<PathBuf>>>,
    #[serde(default)]
    pub visualizer: VisualizerConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
}

impl ProjectConfig {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents = std::fs::read_to_string(path)?;
        let config: ProjectConfig = toml::from_str(&contents)?;
        Ok(config)
    }

    pub fn discover() -> Result<Self> {
        Self::load(Path::new("robowar.toml"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_full_config() {
        let toml = r#"
arena = "arenas/square.toml"
robots = ["spinner.toml", "patrol.toml"]
max_ticks = 5000
seed = 123

[visualizer]
speed = 2.0

[visualizer.keybindings]
pause = "P"
"#;
        let config: ProjectConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.arena.unwrap(), PathBuf::from("arenas/square.toml"));
        assert_eq!(config.robots.unwrap().len(), 2);
        assert_eq!(config.max_ticks.unwrap(), 5000);
        assert_eq!(config.seed.unwrap(), 123);
        assert_eq!(config.visualizer.speed.unwrap(), 2.0);
        assert_eq!(config.visualizer.keybindings.pause, vec!["P"]);
        assert_eq!(config.visualizer.keybindings.speed_up, vec!["Equal"]);
    }

    #[test]
    fn parse_partial_config() {
        let toml = r#"
robots = ["dumb.toml"]
"#;
        let config: ProjectConfig = toml::from_str(toml).unwrap();
        assert!(config.arena.is_none());
        assert_eq!(config.robots.unwrap().len(), 1);
        assert!(config.max_ticks.is_none());
        assert!(config.seed.is_none());
    }

    #[test]
    fn load_nonexistent_returns_default() {
        let config = ProjectConfig::load(Path::new("nonexistent_robowar.toml")).unwrap();
        assert!(config.arena.is_none());
        assert!(config.robots.is_none());
        assert!(config.max_ticks.is_none());
        assert!(config.seed.is_none());
    }

    #[test]
    fn parse_groups() {
        let toml = r#"
robots = ["@movers", "@campers"]

[groups]
movers = ["patrol.toml", "dumb.toml"]
campers = ["spinner.toml"]
"#;
        let config: ProjectConfig = toml::from_str(toml).unwrap();
        let groups = config.groups.unwrap();
        assert_eq!(groups.len(), 2);
        assert_eq!(groups["movers"].len(), 2);
        assert_eq!(groups["movers"][0], PathBuf::from("patrol.toml"));
        assert_eq!(groups["movers"][1], PathBuf::from("dumb.toml"));
        assert_eq!(groups["campers"].len(), 1);
        assert_eq!(groups["campers"][0], PathBuf::from("spinner.toml"));
    }

    #[test]
    fn parse_keybinding_as_array() {
        let toml = r#"
[visualizer.keybindings]
pan_up = ["W", "ArrowUp"]
pan_down = "S"
"#;
        let config: ProjectConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.visualizer.keybindings.pan_up, vec!["W", "ArrowUp"]);
        assert_eq!(config.visualizer.keybindings.pan_down, vec!["S"]);
    }
}
