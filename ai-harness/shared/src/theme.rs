//! The built-in theme documents, user-authored ones, and the pure resolution
//! rules over both.
//!
//! A *theme document* ([`Theme`]) is the whole palette — every CSS custom
//! property `client/style.css` reads — not a light/dark flag. The database
//! stores the user's **choice** ([`THEME_PREFERENCE_KEY`], encoded by
//! [`ThemeChoice::to_stored`]) and, separately, the user's own theme
//! documents (in the `themes` table, DTO'd here as [`UserTheme`]). The eight
//! built-in documents ([`BuiltIn`]) are compiled in and never touch the
//! database at all. That split is what lets the client apply a theme by
//! writing custom properties onto the root element at runtime regardless of
//! where the document came from — see [`resolve`].
//!
//! A theme is colors and nothing else — no border radii, spacing, or
//! typography. Those stay literal in `style.css`, same as today. Which color
//! goes with which CSS variable is defined exactly once, in [`ThemeField`];
//! every other place that needs to walk "all ten fields" (the settings
//! read-only view, the edit form, [`Theme::css_vars`]) loops over
//! [`ThemeField::ALL`] instead of re-listing them.
//!
//! Everything here is a pure function over plain data, deliberately: `client`
//! is wasm-only and excluded from the workspace's `default-members`, so a
//! plain `cargo test` never compiles it. Putting the resolution rules, the
//! variable mapping, and the naming/validation rules here is what makes them
//! testable at all.

use serde::{Deserialize, Serialize};

use crate::ids::ThemeId;

/// The `preferences` row key the theme choice is stored under. Named here so
/// the backend and the client can't drift on it.
pub const THEME_PREFERENCE_KEY: &str = "theme";

/// One of the color slots a [`Theme`] document defines, paired with its CSS
/// custom property name. The single place that mapping is spelled out —
/// [`Theme::css_vars`] and every UI that walks "all the fields" go through
/// [`ThemeField::ALL`] instead of re-listing the ten fields by hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThemeField {
    Fg,
    Bg,
    BgRaised,
    Muted,
    Border,
    Error,
    Accent,
    AccentFg,
    BubbleHumanBg,
    BubbleHumanFg,
}

impl ThemeField {
    pub const ALL: [ThemeField; Theme::VAR_COUNT] = [
        Self::Fg,
        Self::Bg,
        Self::BgRaised,
        Self::Muted,
        Self::Border,
        Self::Error,
        Self::Accent,
        Self::AccentFg,
        Self::BubbleHumanBg,
        Self::BubbleHumanFg,
    ];

    /// The CSS custom property this field is written to, e.g. `--bg-raised`.
    pub fn css_var(self) -> &'static str {
        match self {
            Self::Fg => "--fg",
            Self::Bg => "--bg",
            Self::BgRaised => "--bg-raised",
            Self::Muted => "--muted",
            Self::Border => "--border",
            Self::Error => "--error",
            Self::Accent => "--accent",
            Self::AccentFg => "--accent-fg",
            Self::BubbleHumanBg => "--bubble-human-bg",
            Self::BubbleHumanFg => "--bubble-human-fg",
        }
    }

    /// A human-readable label for the settings read-only view and edit form.
    pub fn label(self) -> &'static str {
        match self {
            Self::Fg => "Text",
            Self::Bg => "Background",
            Self::BgRaised => "Raised background",
            Self::Muted => "Muted text",
            Self::Border => "Border",
            Self::Error => "Error",
            Self::Accent => "Accent",
            Self::AccentFg => "Text on accent",
            Self::BubbleHumanBg => "Message bubble background",
            Self::BubbleHumanFg => "Message bubble text",
        }
    }

    /// Read this field's value out of `theme`.
    pub fn get(self, theme: &Theme) -> &str {
        match self {
            Self::Fg => &theme.fg,
            Self::Bg => &theme.bg,
            Self::BgRaised => &theme.bg_raised,
            Self::Muted => &theme.muted,
            Self::Border => &theme.border,
            Self::Error => &theme.error,
            Self::Accent => &theme.accent,
            Self::AccentFg => &theme.accent_fg,
            Self::BubbleHumanBg => &theme.bubble_human_bg,
            Self::BubbleHumanFg => &theme.bubble_human_fg,
        }
    }

    /// Write this field's value into `theme`.
    pub fn set(self, theme: &mut Theme, value: String) {
        match self {
            Self::Fg => theme.fg = value,
            Self::Bg => theme.bg = value,
            Self::BgRaised => theme.bg_raised = value,
            Self::Muted => theme.muted = value,
            Self::Border => theme.border = value,
            Self::Error => theme.error = value,
            Self::Accent => theme.accent = value,
            Self::AccentFg => theme.accent_fg = value,
            Self::BubbleHumanBg => theme.bubble_human_bg = value,
            Self::BubbleHumanFg => theme.bubble_human_fg = value,
        }
    }
}

/// A whole palette. One field per custom property `client/style.css` declares
/// in `:root`; [`Theme::css_vars`] is the only place the CSS names appear.
///
/// `String` rather than `&'static str` even though every built-in is a
/// compile-time constant — a document loaded from a database row has to fit
/// the same type, and a palette is read once per theme change, not per
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
    /// extending [`ThemeField`] is a compile error, not a color that silently
    /// never gets written.
    pub const VAR_COUNT: usize = 10;

    /// This document as `("--name", "value")` pairs, ready to hand to
    /// `CSSStyleDeclaration.setProperty`.
    ///
    /// Pure and allocation-free, so the field-to-variable mapping is testable
    /// on the host with no DOM — `client::theme::apply` is nothing but a loop
    /// over this.
    pub fn css_vars(&self) -> [(&'static str, &str); Self::VAR_COUNT] {
        ThemeField::ALL.map(|field| (field.css_var(), field.get(self)))
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

/// The registry of compiled-in theme documents. An enum rather than a `const`
/// array — `Theme` holds `String`s, which rules out a `const` — but gives the
/// same exhaustiveness guarantee: a new variant that's missing from a `match`
/// anywhere in this module is a compile error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltIn {
    Light,
    Dark,
    GruvboxLight,
    GruvboxDark,
    Dracula,
    Nord,
    SolarizedLight,
    SolarizedDark,
}

impl BuiltIn {
    pub const ALL: [BuiltIn; 8] = [
        Self::Light,
        Self::Dark,
        Self::GruvboxLight,
        Self::GruvboxDark,
        Self::Dracula,
        Self::Nord,
        Self::SolarizedLight,
        Self::SolarizedDark,
    ];

    /// The exact string stored in `preferences.value` (via
    /// [`ThemeChoice::to_stored`]) and used as an `<option value>` in the
    /// settings picker. Stable across releases — a stored `"light"`/`"dark"`
    /// from before this change must keep resolving to the same choice.
    pub fn id(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
            Self::GruvboxLight => "gruvbox-light",
            Self::GruvboxDark => "gruvbox-dark",
            Self::Dracula => "dracula",
            Self::Nord => "nord",
            Self::SolarizedLight => "solarized-light",
            Self::SolarizedDark => "solarized-dark",
        }
    }

    /// Shown in the theme picker, and the value checked against user-theme
    /// names for a collision — see [`built_in_name_conflict`].
    pub fn label(self) -> &'static str {
        match self {
            Self::Light => "Light",
            Self::Dark => "Dark",
            Self::GruvboxLight => "Gruvbox Light",
            Self::GruvboxDark => "Gruvbox Dark",
            Self::Dracula => "Dracula",
            Self::Nord => "Nord",
            Self::SolarizedLight => "Solarized Light",
            Self::SolarizedDark => "Solarized Dark",
        }
    }

    /// This built-in's palette.
    pub fn theme(self) -> Theme {
        match self {
            Self::Light => Theme::light(),
            Self::Dark => Theme::dark(),
            Self::GruvboxLight => Theme {
                fg: "#3c3836".to_string(),
                bg: "#fbf1c7".to_string(),
                bg_raised: "#ebdbb2".to_string(),
                muted: "#7c6f64".to_string(),
                border: "#d5c4a1".to_string(),
                error: "#9d0006".to_string(),
                accent: "#076678".to_string(),
                accent_fg: "#fbf1c7".to_string(),
                bubble_human_bg: "#076678".to_string(),
                bubble_human_fg: "#fbf1c7".to_string(),
            },
            Self::GruvboxDark => Theme {
                fg: "#ebdbb2".to_string(),
                bg: "#282828".to_string(),
                bg_raised: "#3c3836".to_string(),
                muted: "#a89984".to_string(),
                border: "#504945".to_string(),
                error: "#fb4934".to_string(),
                accent: "#83a598".to_string(),
                accent_fg: "#282828".to_string(),
                bubble_human_bg: "#458588".to_string(),
                bubble_human_fg: "#ebdbb2".to_string(),
            },
            Self::Dracula => Theme {
                fg: "#f8f8f2".to_string(),
                bg: "#282a36".to_string(),
                bg_raised: "#343746".to_string(),
                muted: "#6272a4".to_string(),
                border: "#44475a".to_string(),
                error: "#ff5555".to_string(),
                accent: "#bd93f9".to_string(),
                accent_fg: "#282a36".to_string(),
                bubble_human_bg: "#6272a4".to_string(),
                bubble_human_fg: "#f8f8f2".to_string(),
            },
            Self::Nord => Theme {
                fg: "#eceff4".to_string(),
                bg: "#2e3440".to_string(),
                bg_raised: "#3b4252".to_string(),
                muted: "#a3adc2".to_string(),
                border: "#434c5e".to_string(),
                error: "#bf616a".to_string(),
                accent: "#88c0d0".to_string(),
                accent_fg: "#2e3440".to_string(),
                bubble_human_bg: "#5e81ac".to_string(),
                bubble_human_fg: "#eceff4".to_string(),
            },
            Self::SolarizedLight => Theme {
                fg: "#586e75".to_string(),
                bg: "#fdf6e3".to_string(),
                bg_raised: "#eee8d5".to_string(),
                muted: "#93a1a1".to_string(),
                border: "#ded8c5".to_string(),
                error: "#dc322f".to_string(),
                accent: "#268bd2".to_string(),
                accent_fg: "#fdf6e3".to_string(),
                bubble_human_bg: "#268bd2".to_string(),
                bubble_human_fg: "#fdf6e3".to_string(),
            },
            Self::SolarizedDark => Theme {
                fg: "#93a1a1".to_string(),
                bg: "#002b36".to_string(),
                bg_raised: "#073642".to_string(),
                muted: "#657b83".to_string(),
                border: "#0f4b5a".to_string(),
                error: "#dc322f".to_string(),
                accent: "#268bd2".to_string(),
                accent_fg: "#002b36".to_string(),
                bubble_human_bg: "#268bd2".to_string(),
                bubble_human_fg: "#fdf6e3".to_string(),
            },
        }
    }

    /// Look up a built-in by its stored id. `None` for anything that isn't
    /// exactly one of [`BuiltIn::ALL`]'s ids.
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|built_in| built_in.id() == id)
    }
}

/// What the user picked. Not a theme — a *selector* for one: `System`
/// resolves against the live OS preference, `BuiltIn` and `User` name a
/// specific document. See [`resolve`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeChoice {
    /// Follow `prefers-color-scheme`, and keep following it if the OS flips.
    System,
    BuiltIn(BuiltIn),
    /// A row in the `themes` table.
    User(ThemeId),
}

impl Default for ThemeChoice {
    fn default() -> Self {
        Self::System
    }
}

impl ThemeChoice {
    /// The exact string stored in `preferences.value` and used as the
    /// `<option value>` in the settings picker — matches
    /// [`ThemeChoice::from_stored`], so there is exactly one encoding of a
    /// choice.
    pub fn to_stored(self) -> String {
        match self {
            Self::System => "system".to_string(),
            Self::BuiltIn(built_in) => built_in.id().to_string(),
            Self::User(id) => format!("user:{}", id.get()),
        }
    }

    /// Read a stored (or `<select>`) value back.
    ///
    /// `None` — no row yet — and an unrecognized value both fall back to
    /// `System`. An unrecognized value isn't an error worth surfacing: it
    /// means a deleted user theme, a downgrade, or a hand-edited database,
    /// and the sane response to any of those is to look like the rest of the
    /// desktop.
    pub fn from_stored(value: Option<&str>) -> Self {
        match value {
            None | Some("system") => Self::System,
            Some(rest) => {
                if let Some(built_in) = BuiltIn::from_id(rest) {
                    Self::BuiltIn(built_in)
                } else if let Some(id) = rest.strip_prefix("user:").and_then(|s| s.parse::<i64>().ok()) {
                    Self::User(ThemeId::new(id))
                } else {
                    Self::System
                }
            }
        }
    }
}

/// A user-authored theme document, as stored in the `themes` table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserTheme {
    pub id: ThemeId,
    pub name: String,
    pub theme: Theme,
    pub created_at: String,
    pub updated_at: String,
}

/// The editable fields of a [`UserTheme`] — what `create_theme`/`update_theme`
/// take, mirroring `shared::agent::AgentConfigInput`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserThemeInput {
    pub name: String,
    pub theme: Theme,
}

/// Parse a `#rrggbb` string. Anything else — `#rgb` shorthand, a named color,
/// garbage — is `None`. Used both as [`Theme::is_dark`]'s best-effort parse
/// and as the color-field validator for a user-authored theme.
pub fn parse_hex_rgb(value: &str) -> Option<(u8, u8, u8)> {
    let hex = value.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some((r, g, b))
}

/// Whether `name` collides with a built-in theme's label, case-insensitively.
/// The database's unique index on `themes.name` only sees other user themes,
/// so this is the other half of "a theme name must be unique across every
/// theme, built in and user-provided."
pub fn built_in_name_conflict(name: &str) -> Option<BuiltIn> {
    BuiltIn::ALL.into_iter().find(|built_in| built_in.label().eq_ignore_ascii_case(name))
}

/// The one rule that turns a choice, the current OS preference, and the
/// user's own theme catalog into a document. Lives here, not in `client`, so
/// `cargo test` covers it.
///
/// A `User` choice whose id isn't in `user` — a theme deleted out from under
/// the stored preference, or a hand-edited database — falls back to the
/// `System` result rather than erroring: the honest answer when what was
/// picked no longer exists.
pub fn resolve(choice: ThemeChoice, system_prefers_dark: bool, user: &[UserTheme]) -> Theme {
    match choice {
        ThemeChoice::System => {
            if system_prefers_dark {
                BuiltIn::Dark.theme()
            } else {
                BuiltIn::Light.theme()
            }
        }
        ThemeChoice::BuiltIn(built_in) => built_in.theme(),
        ThemeChoice::User(id) => match user.iter().find(|stored| stored.id == id) {
            Some(stored) => stored.theme.clone(),
            None => resolve(ThemeChoice::System, system_prefers_dark, user),
        },
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
        // Deliberately scoped to `light()`/`dark()`, not `BuiltIn::ALL`:
        // `client/style.css`'s bare `:root` and its `prefers-color-scheme:
        // dark` block are the pre-wasm-boot fallback for exactly these two
        // documents. The other six built-ins are never painted before wasm
        // boots, so they have no stylesheet counterpart to match — don't
        // widen this loop to `BuiltIn::ALL` when it's next touched.
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
        assert_eq!(resolve(ThemeChoice::System, true, &[]), Theme::dark());
        assert_eq!(resolve(ThemeChoice::System, false, &[]), Theme::light());
    }

    #[test]
    fn an_explicit_built_in_choice_ignores_the_os_preference() {
        assert_eq!(resolve(ThemeChoice::BuiltIn(BuiltIn::Light), true, &[]), Theme::light());
        assert_eq!(resolve(ThemeChoice::BuiltIn(BuiltIn::Dark), false, &[]), Theme::dark());
    }

    #[test]
    fn a_missing_or_unrecognized_preference_falls_back_to_system() {
        assert_eq!(ThemeChoice::from_stored(None), ThemeChoice::System);
        assert_eq!(ThemeChoice::from_stored(Some("solarized")), ThemeChoice::System);
    }

    #[test]
    fn built_in_ids_round_trip_through_to_stored_and_from_stored() {
        for built_in in BuiltIn::ALL {
            let choice = ThemeChoice::BuiltIn(built_in);
            assert_eq!(ThemeChoice::from_stored(Some(&choice.to_stored())), choice);
        }
    }

    #[test]
    fn existing_installs_stored_light_or_dark_still_parse() {
        // Values written by the app before this change must keep resolving
        // to the same built-in choices, not reset to System.
        assert_eq!(ThemeChoice::from_stored(Some("light")), ThemeChoice::BuiltIn(BuiltIn::Light));
        assert_eq!(ThemeChoice::from_stored(Some("dark")), ThemeChoice::BuiltIn(BuiltIn::Dark));
    }

    #[test]
    fn a_user_choice_round_trips_through_to_stored_and_from_stored() {
        let choice = ThemeChoice::User(ThemeId::new(42));
        assert_eq!(choice.to_stored(), "user:42");
        assert_eq!(ThemeChoice::from_stored(Some("user:42")), choice);
    }

    #[test]
    fn a_malformed_user_choice_falls_back_to_system() {
        assert_eq!(ThemeChoice::from_stored(Some("user:")), ThemeChoice::System);
        assert_eq!(ThemeChoice::from_stored(Some("user:abc")), ThemeChoice::System);
    }

    fn sample_user_theme(id: i64, name: &str) -> UserTheme {
        UserTheme {
            id: ThemeId::new(id),
            name: name.to_string(),
            theme: BuiltIn::Dracula.theme(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn resolve_returns_the_stored_document_for_a_user_theme_that_exists() {
        let user = vec![sample_user_theme(1, "mine")];
        assert_eq!(resolve(ThemeChoice::User(ThemeId::new(1)), false, &user), BuiltIn::Dracula.theme());
    }

    #[test]
    fn resolve_falls_back_to_system_when_the_chosen_user_theme_is_missing() {
        let user = vec![sample_user_theme(1, "mine")];
        assert_eq!(resolve(ThemeChoice::User(ThemeId::new(999)), false, &user), Theme::light());
        assert_eq!(resolve(ThemeChoice::User(ThemeId::new(999)), true, &user), Theme::dark());
    }

    #[test]
    fn theme_field_set_then_get_round_trips_every_field() {
        for field in ThemeField::ALL {
            let mut theme = Theme::light();
            field.set(&mut theme, "#123456".to_string());
            assert_eq!(field.get(&theme), "#123456");
        }
    }

    #[test]
    fn built_in_ids_and_labels_are_unique() {
        let mut ids: Vec<&str> = BuiltIn::ALL.iter().map(|b| b.id()).collect();
        let mut labels: Vec<&str> = BuiltIn::ALL.iter().map(|b| b.label()).collect();
        let (ids_len, labels_len) = (ids.len(), labels.len());
        ids.sort_unstable();
        ids.dedup();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(ids.len(), ids_len, "duplicate built-in id");
        assert_eq!(labels.len(), labels_len, "duplicate built-in label");
    }

    #[test]
    fn built_in_colors_are_valid_hex() {
        for built_in in BuiltIn::ALL {
            let theme = built_in.theme();
            for field in ThemeField::ALL {
                let value = field.get(&theme);
                assert!(
                    parse_hex_rgb(value).is_some(),
                    "{built_in:?}'s {field:?} = {value:?} is not #rrggbb"
                );
            }
        }
    }

    #[test]
    fn built_in_name_conflict_is_case_insensitive_and_none_for_a_novel_name() {
        assert_eq!(built_in_name_conflict("dracula"), Some(BuiltIn::Dracula));
        assert_eq!(built_in_name_conflict("DRACULA"), Some(BuiltIn::Dracula));
        assert_eq!(built_in_name_conflict("Dracula"), Some(BuiltIn::Dracula));
        assert_eq!(built_in_name_conflict("my custom theme"), None);
    }

    #[test]
    fn parse_hex_rgb_accepts_rrggbb_and_rejects_everything_else() {
        assert_eq!(parse_hex_rgb("#2563eb"), Some((0x25, 0x63, 0xeb)));
        assert_eq!(parse_hex_rgb("#FFFFFF"), Some((255, 255, 255)));
        assert_eq!(parse_hex_rgb("2563eb"), None); // missing '#'
        assert_eq!(parse_hex_rgb("#fff"), None); // shorthand not supported
        assert_eq!(parse_hex_rgb("papayawhip"), None);
        assert_eq!(parse_hex_rgb("#gggggg"), None);
    }

    #[test]
    fn is_dark_classifies_every_built_in_document_correctly() {
        let expected_dark = [
            (BuiltIn::Light, false),
            (BuiltIn::Dark, true),
            (BuiltIn::GruvboxLight, false),
            (BuiltIn::GruvboxDark, true),
            (BuiltIn::Dracula, true),
            (BuiltIn::Nord, true),
            (BuiltIn::SolarizedLight, false),
            (BuiltIn::SolarizedDark, true),
        ];
        for (built_in, dark) in expected_dark {
            assert_eq!(built_in.theme().is_dark(), dark, "{built_in:?}");
        }
    }

    #[test]
    fn is_dark_falls_back_to_light_on_an_unparseable_background() {
        let mut theme = Theme::light();
        theme.bg = "papayawhip".to_string();
        assert!(!theme.is_dark());
    }
}
