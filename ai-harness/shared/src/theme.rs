//! The built-in theme documents, and the pure resolution rules over them.
//!
//! A *theme document* is the whole palette — every CSS custom property
//! `client/style.css` reads — not a light/dark flag. The database stores only
//! the user's **choice by name** ([`THEME_PREFERENCE_KEY`] = `system` |
//! `light` | `dark`); the documents themselves are compiled in here. That
//! split is what lets the client apply a theme by writing custom properties
//! onto the root element at runtime, and what leaves room for user-authored
//! documents later without changing how a theme is applied.
//!
//! A theme is colors and nothing else — no border radii, spacing, or
//! typography. Those stay literal in `style.css`, same as today.
//!
//! Everything here is a pure function over plain data, deliberately: `client`
//! is wasm-only and excluded from the workspace's `default-members`, so a
//! plain `cargo test` never compiles it. Putting the resolution rules and the
//! variable mapping here is what makes them testable at all.

use serde::{Deserialize, Serialize};

/// The `preferences` row key the theme choice is stored under. Named here so
/// the backend and the client can't drift on it.
pub const THEME_PREFERENCE_KEY: &str = "theme";

/// What the user picked. Not a theme — a *selector* for one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeChoice {
    /// Follow `prefers-color-scheme`, and keep following it if the OS flips.
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeChoice {
    pub const ALL: [ThemeChoice; 3] = [Self::System, Self::Light, Self::Dark];

    /// The exact string stored in `preferences.value` and used as the
    /// `<option value>` in the settings picker — matches the serde
    /// representation, so there is exactly one encoding of a choice.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    /// Read a stored (or `<select>`) value back.
    ///
    /// `None` — no row yet — and an unrecognized value both fall back to
    /// `System`. An unrecognized value isn't an error worth surfacing: it
    /// means a downgrade or a hand-edited database, and the sane response to
    /// either is to look like the rest of the desktop.
    pub fn from_stored(value: Option<&str>) -> Self {
        match value {
            Some("light") => Self::Light,
            Some("dark") => Self::Dark,
            _ => Self::System,
        }
    }
}

/// A whole palette. One field per custom property `client/style.css` declares
/// in `:root`; [`Theme::css_vars`] is the only place the CSS names appear.
///
/// `String` rather than `&'static str` even though both built-ins are
/// compile-time constants — a document loaded from a database row later has
/// to fit the same type, and a palette is read once per theme change, not per
/// render.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Theme {
    pub fg: String,
    pub bg: String,
    pub bg_raised: String,
    pub muted: String,
    pub border: String,
    pub error: String,
    pub accent: String,
    pub accent_fg: String,
    pub bubble_human_bg: String,
    pub bubble_human_fg: String,
}

impl Theme {
    /// How many custom properties a document defines. A fixed-size array
    /// return type from [`Theme::css_vars`] means adding a field without
    /// extending that mapping is a compile error, not a color that silently
    /// never gets written.
    pub const VAR_COUNT: usize = 10;

    /// This document as `("--name", "value")` pairs, ready to hand to
    /// `CSSStyleDeclaration.setProperty`.
    ///
    /// Pure and allocation-free, so the field-to-variable mapping is testable
    /// on the host with no DOM — `client::theme::apply` is nothing but a loop
    /// over this.
    pub fn css_vars(&self) -> [(&'static str, &str); Self::VAR_COUNT] {
        [
            ("--fg", self.fg.as_str()),
            ("--bg", self.bg.as_str()),
            ("--bg-raised", self.bg_raised.as_str()),
            ("--muted", self.muted.as_str()),
            ("--border", self.border.as_str()),
            ("--error", self.error.as_str()),
            ("--accent", self.accent.as_str()),
            ("--accent-fg", self.accent_fg.as_str()),
            ("--bubble-human-bg", self.bubble_human_bg.as_str()),
            ("--bubble-human-fg", self.bubble_human_fg.as_str()),
        ]
    }

    /// The built-in light palette — the values `client/style.css` declares in
    /// its bare `:root` block, which is the pre-wasm-boot fallback.
    pub fn light() -> Self {
        Self {
            fg: "#1a1a1a".to_string(),
            bg: "#ffffff".to_string(),
            bg_raised: "#f5f5f5".to_string(),
            muted: "#666666".to_string(),
            border: "#dddddd".to_string(),
            error: "#b3261e".to_string(),
            accent: "#2563eb".to_string(),
            accent_fg: "#ffffff".to_string(),
            bubble_human_bg: "#2563eb".to_string(),
            bubble_human_fg: "#ffffff".to_string(),
        }
    }

    /// The built-in dark palette — the values in the stylesheet's
    /// `@media (prefers-color-scheme: dark)` block.
    pub fn dark() -> Self {
        Self {
            fg: "#e8e8e8".to_string(),
            bg: "#1e1e1e".to_string(),
            bg_raised: "#282828".to_string(),
            muted: "#9a9a9a".to_string(),
            border: "#3a3a3a".to_string(),
            error: "#ff8a80".to_string(),
            accent: "#5b8def".to_string(),
            accent_fg: "#0b1220".to_string(),
            bubble_human_bg: "#3f6fd1".to_string(),
            bubble_human_fg: "#ffffff".to_string(),
        }
    }

    /// Whether this document reads as a dark theme, derived from `bg`'s
    /// perceived brightness rather than stored as a field — a theme document
    /// is colors and nothing else, and this keeps it that way.
    ///
    /// Drives the `color-scheme` property the client writes alongside the
    /// custom properties: without it, an explicit Light choice on a dark OS
    /// renders light content inside dark scrollbars, caret, and form
    /// controls. An unparseable `bg` reads as light, matching the stylesheet's
    /// own bare `:root` default.
    pub fn is_dark(&self) -> bool {
        match parse_hex_rgb(&self.bg) {
            Some((r, g, b)) => {
                // Perceived brightness (ITU-R BT.601), 0-255 scale; below the
                // midpoint reads as a dark background.
                let brightness = (r as u32 * 299 + g as u32 * 587 + b as u32 * 114) / 1000;
                brightness < 128
            }
            None => false,
        }
    }
}

/// Parse a `#rrggbb` string. Anything else — `#rgb` shorthand, a named color,
/// garbage — is `None` rather than an error: [`Theme::is_dark`] is a
/// best-effort hint for native chrome, not a correctness requirement.
fn parse_hex_rgb(value: &str) -> Option<(u8, u8, u8)> {
    let hex = value.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some((r, g, b))
}

/// The one rule that turns a choice plus the current OS preference into a
/// document. Lives here, not in `client`, so `cargo test` covers it.
pub fn resolve(choice: ThemeChoice, system_prefers_dark: bool) -> Theme {
    match choice {
        ThemeChoice::Light => Theme::light(),
        ThemeChoice::Dark => Theme::dark(),
        ThemeChoice::System if system_prefers_dark => Theme::dark(),
        ThemeChoice::System => Theme::light(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pulled in at *compile* time, and only for test builds — the wasm build
    /// of this crate never sees it, so the "no `std::fs`" rule in the crate
    /// doc is intact. The coupling to a path in a sibling crate is
    /// deliberate: the stylesheet is the other half of this module's
    /// contract, and this is what catches the two drifting apart.
    const STYLESHEET: &str = include_str!("../../client/style.css");

    /// The custom-property names declared in the `nth` `:root { … }` block —
    /// `0` is the light default, `1` the `prefers-color-scheme: dark`
    /// override.
    fn root_block_vars(nth: usize) -> Vec<&'static str> {
        let (start, marker) = STYLESHEET
            .match_indices(":root {")
            .nth(nth)
            .expect("client/style.css declares a light `:root` and a dark one");
        let body = &STYLESHEET[start + marker.len()..];
        let body = &body[..body.find('}').expect("unterminated `:root` block")];
        let mut names: Vec<&str> = body
            .lines()
            .filter_map(|line| line.trim().split(':').next())
            .filter(|name| name.starts_with("--"))
            .collect();
        names.sort_unstable();
        names
    }

    #[test]
    fn css_vars_names_are_exactly_what_the_stylesheet_declares() {
        let mut from_rust: Vec<&str> =
            Theme::light().css_vars().iter().map(|(name, _)| *name).collect();
        from_rust.sort_unstable();
        assert_eq!(from_rust, root_block_vars(0));
    }

    #[test]
    fn the_dark_fallback_block_declares_the_same_names_as_the_light_default() {
        // A name in one block but not the other is the exact way an explicit
        // Light choice on a dark OS ends up with one stray dark color.
        assert_eq!(root_block_vars(0), root_block_vars(1));
    }

    #[test]
    fn the_built_in_documents_match_the_stylesheet_values() {
        for theme in [Theme::light(), Theme::dark()] {
            for (name, value) in theme.css_vars() {
                let needle = format!("{name}: {value};");
                assert!(
                    STYLESHEET.contains(&needle),
                    "`{needle}` is not in client/style.css, so the pre-boot \
                     fallback and the runtime document disagree"
                );
            }
        }
    }

    #[test]
    fn system_follows_the_os_preference() {
        assert_eq!(resolve(ThemeChoice::System, true), Theme::dark());
        assert_eq!(resolve(ThemeChoice::System, false), Theme::light());
    }

    #[test]
    fn an_explicit_choice_ignores_the_os_preference() {
        assert_eq!(resolve(ThemeChoice::Light, true), Theme::light());
        assert_eq!(resolve(ThemeChoice::Dark, false), Theme::dark());
    }

    #[test]
    fn a_missing_or_unrecognized_preference_falls_back_to_system() {
        assert_eq!(ThemeChoice::from_stored(None), ThemeChoice::System);
        assert_eq!(ThemeChoice::from_stored(Some("solarized")), ThemeChoice::System);
    }

    #[test]
    fn the_stored_value_round_trips_and_matches_the_json_representation() {
        for choice in ThemeChoice::ALL {
            assert_eq!(ThemeChoice::from_stored(Some(choice.as_str())), choice);
            assert_eq!(serde_json::to_value(choice).unwrap(), serde_json::json!(choice.as_str()));
        }
    }

    #[test]
    fn is_dark_classifies_the_built_in_documents_correctly() {
        assert!(!Theme::light().is_dark());
        assert!(Theme::dark().is_dark());
    }

    #[test]
    fn is_dark_falls_back_to_light_on_an_unparseable_background() {
        let mut theme = Theme::light();
        theme.bg = "papayawhip".to_string();
        assert!(!theme.is_dark());
    }
}
