//! Persistent UI preferences (panel locations, visibility, active tabs).
//!
//! Lives at the path configured in `config.toml` (default
//! `<config_dir>/state.toml`). Read once on startup; written synchronously
//! every time a tracked toggle changes. Unknown keys are logged and dropped
//! on the next rewrite.

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

const KNOWN_KEYS: &[&str] = &[
    "derived_location",
    "derived_visible",
    "validation_location",
    "validation_visible",
    "actions_location",
    "actions_visible",
    "dicelog_location",
    "dicelog_visible",
    "left_active",
    "right_active",
    "bottom_active",
    "theme_preference",
];

/// User's choice of color theme.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    Light,
    Dark,
    #[default]
    System,
}

impl ThemePreference {
    pub const ALL: [ThemePreference; 3] = [
        ThemePreference::System,
        ThemePreference::Dark,
        ThemePreference::Light,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ThemePreference::Light => "Light",
            ThemePreference::Dark => "Dark",
            ThemePreference::System => "System",
        }
    }
}

impl From<ThemePreference> for egui::ThemePreference {
    fn from(value: ThemePreference) -> Self {
        match value {
            ThemePreference::Light => egui::ThemePreference::Light,
            ThemePreference::Dark => egui::ThemePreference::Dark,
            ThemePreference::System => egui::ThemePreference::System,
        }
    }
}

/// Where a dockable item can live.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PanelLocation {
    Left,
    Right,
    Bottom,
}

impl PanelLocation {
    pub const ALL: [PanelLocation; 3] = [
        PanelLocation::Left,
        PanelLocation::Right,
        PanelLocation::Bottom,
    ];

    pub fn label(self) -> &'static str {
        match self {
            PanelLocation::Left => "Left sidebar",
            PanelLocation::Right => "Right sidebar",
            PanelLocation::Bottom => "Bottom panel",
        }
    }
}

/// A dockable item that can be moved between locations and tabbed with
/// other items in the same location.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PanelItem {
    Derived,
    Actions,
    Validation,
    DiceLog,
}

impl PanelItem {
    /// Stable presentation order when multiple items share a panel.
    pub const ORDER: [PanelItem; 4] = [
        PanelItem::Derived,
        PanelItem::Actions,
        PanelItem::Validation,
        PanelItem::DiceLog,
    ];

    pub fn label(self) -> &'static str {
        match self {
            PanelItem::Derived => "Derived",
            PanelItem::Actions => "Actions",
            PanelItem::Validation => "Validation",
            PanelItem::DiceLog => "Dice Log",
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

fn default_derived_location() -> PanelLocation {
    PanelLocation::Right
}

fn default_validation_location() -> PanelLocation {
    PanelLocation::Bottom
}

fn default_actions_location() -> PanelLocation {
    PanelLocation::Right
}

fn default_dicelog_location() -> PanelLocation {
    PanelLocation::Bottom
}

#[derive(Serialize, Deserialize)]
struct UiStateFile {
    #[serde(default = "default_derived_location")]
    derived_location: PanelLocation,
    #[serde(default = "default_true")]
    derived_visible: bool,
    #[serde(default = "default_validation_location")]
    validation_location: PanelLocation,
    #[serde(default = "default_true")]
    validation_visible: bool,
    #[serde(default = "default_actions_location")]
    actions_location: PanelLocation,
    #[serde(default = "default_true")]
    actions_visible: bool,
    #[serde(default = "default_dicelog_location")]
    dicelog_location: PanelLocation,
    #[serde(default = "default_false")]
    dicelog_visible: bool,
    #[serde(default)]
    left_active: Option<PanelItem>,
    #[serde(default)]
    right_active: Option<PanelItem>,
    #[serde(default)]
    bottom_active: Option<PanelItem>,
    #[serde(default)]
    theme_preference: ThemePreference,
}

impl Default for UiStateFile {
    fn default() -> Self {
        Self {
            derived_location: default_derived_location(),
            derived_visible: true,
            validation_location: default_validation_location(),
            validation_visible: true,
            actions_location: default_actions_location(),
            actions_visible: true,
            dicelog_location: default_dicelog_location(),
            dicelog_visible: false,
            left_active: None,
            right_active: None,
            bottom_active: None,
            theme_preference: ThemePreference::default(),
        }
    }
}

pub struct UiState {
    path: PathBuf,
    pub derived_location: PanelLocation,
    pub derived_visible: bool,
    pub validation_location: PanelLocation,
    pub validation_visible: bool,
    pub actions_location: PanelLocation,
    pub actions_visible: bool,
    pub dicelog_location: PanelLocation,
    pub dicelog_visible: bool,
    pub left_active: Option<PanelItem>,
    pub right_active: Option<PanelItem>,
    pub bottom_active: Option<PanelItem>,
    pub theme_preference: ThemePreference,
}

impl UiState {
    pub fn load_or_default(path: PathBuf) -> Self {
        let text = match fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Self::from_file(path, UiStateFile::default());
            }
            Err(e) => {
                tracing::warn!(
                    "could not read UI state file {}: {} (using defaults)",
                    path.display(),
                    e
                );
                return Self::from_file(path, UiStateFile::default());
            }
        };

        warn_unknown_keys(&path, &text);

        let parsed: UiStateFile = match toml::from_str(&text) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(
                    "could not parse UI state file {}: {} (using defaults)",
                    path.display(),
                    e
                );
                UiStateFile::default()
            }
        };

        Self::from_file(path, parsed)
    }

    fn from_file(path: PathBuf, file: UiStateFile) -> Self {
        Self {
            path,
            derived_location: file.derived_location,
            derived_visible: file.derived_visible,
            validation_location: file.validation_location,
            validation_visible: file.validation_visible,
            actions_location: file.actions_location,
            actions_visible: file.actions_visible,
            dicelog_location: file.dicelog_location,
            dicelog_visible: file.dicelog_visible,
            left_active: file.left_active,
            right_active: file.right_active,
            bottom_active: file.bottom_active,
            theme_preference: file.theme_preference,
        }
    }

    pub fn location_of(&self, item: PanelItem) -> PanelLocation {
        match item {
            PanelItem::Derived => self.derived_location,
            PanelItem::Validation => self.validation_location,
            PanelItem::Actions => self.actions_location,
            PanelItem::DiceLog => self.dicelog_location,
        }
    }

    pub fn is_visible(&self, item: PanelItem) -> bool {
        match item {
            PanelItem::Derived => self.derived_visible,
            PanelItem::Validation => self.validation_visible,
            PanelItem::Actions => self.actions_visible,
            PanelItem::DiceLog => self.dicelog_visible,
        }
    }

    pub fn set_visible(&mut self, item: PanelItem, visible: bool) {
        if self.is_visible(item) == visible {
            return;
        }
        tracing::debug!(panel = ?item, visible, "panel visibility toggled");
        match item {
            PanelItem::Derived => self.derived_visible = visible,
            PanelItem::Validation => self.validation_visible = visible,
            PanelItem::Actions => self.actions_visible = visible,
            PanelItem::DiceLog => self.dicelog_visible = visible,
        }
    }

    pub fn set_location(&mut self, item: PanelItem, location: PanelLocation) {
        if self.location_of(item) == location {
            return;
        }
        tracing::debug!(panel = ?item, location = ?location, "panel relocated");
        match item {
            PanelItem::Derived => self.derived_location = location,
            PanelItem::Validation => self.validation_location = location,
            PanelItem::Actions => self.actions_location = location,
            PanelItem::DiceLog => self.dicelog_location = location,
        }
    }

    pub fn active(&self, location: PanelLocation) -> Option<PanelItem> {
        match location {
            PanelLocation::Left => self.left_active,
            PanelLocation::Right => self.right_active,
            PanelLocation::Bottom => self.bottom_active,
        }
    }

    pub fn set_active(&mut self, location: PanelLocation, item: Option<PanelItem>) {
        if self.active(location) == item {
            return;
        }
        tracing::debug!(location = ?location, active = ?item, "panel active tab changed");
        match location {
            PanelLocation::Left => self.left_active = item,
            PanelLocation::Right => self.right_active = item,
            PanelLocation::Bottom => self.bottom_active = item,
        }
    }

    pub fn set_theme_preference(&mut self, pref: ThemePreference) {
        if self.theme_preference == pref {
            return;
        }
        tracing::debug!(theme = ?pref, "theme preference changed");
        self.theme_preference = pref;
    }

    /// Make `item` visible and set it as the active tab in its current
    /// location, then persist. Used by the Actions panel to surface the
    /// Dice Log automatically when the user clicks a roll button.
    pub fn focus_panel(&mut self, item: PanelItem) {
        let loc = self.location_of(item);
        self.set_visible(item, true);
        self.set_active(loc, Some(item));
        self.save();
    }

    pub fn save(&self) {
        let file = UiStateFile {
            derived_location: self.derived_location,
            derived_visible: self.derived_visible,
            validation_location: self.validation_location,
            validation_visible: self.validation_visible,
            actions_location: self.actions_location,
            actions_visible: self.actions_visible,
            dicelog_location: self.dicelog_location,
            dicelog_visible: self.dicelog_visible,
            left_active: self.left_active,
            right_active: self.right_active,
            bottom_active: self.bottom_active,
            theme_preference: self.theme_preference,
        };
        let text = match toml::to_string_pretty(&file) {
            Ok(t) => t,
            Err(e) => {
                tracing::error!("could not serialize UI state: {}", e);
                return;
            }
        };
        if let Some(parent) = self.path.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                tracing::error!(
                    "could not create directory {} for UI state: {}",
                    parent.display(),
                    e
                );
                return;
            }
        }
        if let Err(e) = fs::write(&self.path, text.as_bytes()) {
            tracing::error!("could not write UI state to {}: {}", self.path.display(), e);
        }
    }
}

fn warn_unknown_keys(path: &std::path::Path, text: &str) {
    let Ok(table): Result<toml::Table, _> = toml::from_str(text) else {
        return;
    };
    let known: HashSet<&str> = KNOWN_KEYS.iter().copied().collect();
    for key in table.keys() {
        if !known.contains(key.as_str()) {
            tracing::warn!(
                "unrecognized key `{}` in UI state file {} — ignoring (will be removed on next save)",
                key,
                path.display()
            );
        }
    }
}
