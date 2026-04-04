use chess_shared::{BoardColors, PieceStyle, PieceStyleMode, Theme, ThemeName};
use leptos::prelude::*;

const STORAGE_KEY: &str = "chess_theme";

pub fn default_classic_theme() -> Theme {
    Theme {
        name: ThemeName::try_from("Classic").unwrap(),
        board: BoardColors {
            light_square: "#f0d9b5".into(),
            dark_square: "#b58863".into(),
        },
        pieces: PieceStyle {
            mode: PieceStyleMode::Circle,
            white_fill: Some("#ffffff".into()),
            black_fill: Some("#333333".into()),
            white_text: Some("#333333".into()),
            black_text: Some("#ffffff".into()),
            svg_base_path: None,
            white_tint: None,
            black_tint: None,
        },
    }
}

pub fn default_standard_theme() -> Theme {
    Theme {
        name: ThemeName::try_from("Standard").unwrap(),
        board: BoardColors {
            light_square: "#f0d9b5".into(),
            dark_square: "#b58863".into(),
        },
        pieces: PieceStyle {
            mode: PieceStyleMode::Svg,
            white_fill: None,
            black_fill: None,
            white_text: None,
            black_text: None,
            svg_base_path: Some("/standard-theme".into()),
            white_tint: None,
            black_tint: None,
        },
    }
}

#[derive(Clone, Copy)]
pub struct ThemeState {
    pub theme: RwSignal<Theme>,
}

impl Default for ThemeState {
    fn default() -> Self {
        Self::new()
    }
}

impl ThemeState {
    pub fn new() -> Self {
        let theme = load_from_storage().unwrap_or_else(default_classic_theme);
        ThemeState {
            theme: RwSignal::new(theme),
        }
    }

    pub fn set(&self, theme: Theme) {
        save_to_storage(&theme);
        self.theme.set(theme);
    }
}

fn load_from_storage() -> Option<Theme> {
    let storage = web_sys::window()?.local_storage().ok()??;
    let json = storage.get_item(STORAGE_KEY).ok()??;
    serde_json::from_str(&json).ok()
}

fn save_to_storage(theme: &Theme) {
    if let Some(storage) = web_sys::window()
        .and_then(|w| w.local_storage().ok())
        .flatten()
    {
        if let Ok(json) = serde_json::to_string(theme) {
            let _ = storage.set_item(STORAGE_KEY, &json);
        }
    }
}

pub fn clear_storage() {
    if let Some(storage) = web_sys::window()
        .and_then(|w| w.local_storage().ok())
        .flatten()
    {
        let _ = storage.remove_item(STORAGE_KEY);
    }
}
