//! User preferences, on top of `persist::Persisted`. Add one by adding a field to `Prefs` and one
//! line to `Prefs::load`.

use shared::library::Library;
use crate::persist::Persisted;
use leptos::prelude::*;
use leptos::wasm_bindgen::closure::Closure;
use leptos::wasm_bindgen::JsCast;
use leptos::web_sys;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolved {
    Light,
    Dark,
}

impl Theme {
    pub fn resolve(self, system_prefers_dark: bool) -> Resolved {
        match self {
            Theme::System if system_prefers_dark => Resolved::Dark,
            Theme::System => Resolved::Light,
            Theme::Light => Resolved::Light,
            Theme::Dark => Resolved::Dark,
        }
    }
}

impl Resolved {
    fn attr_value(self) -> &'static str {
        match self {
            Resolved::Light => "light",
            Resolved::Dark => "dark",
        }
    }
}

fn media_query() -> Option<web_sys::MediaQueryList> {
    web_sys::window()?.match_media("(prefers-color-scheme: dark)").ok()?
}

/// Resolves `theme` against the OS/browser color scheme and reflects it onto `<html
/// data-theme>`, which `styles.css` keys its light/dark palettes off. Watches the media query
/// live so `Theme::System` follows the OS without a reload.
fn install_theme(theme: Persisted<Theme>) {
    let query = media_query();
    let system_dark = RwSignal::new(query.as_ref().map(web_sys::MediaQueryList::matches).unwrap_or(false));

    if let Some(query) = query {
        let on_change = Closure::<dyn FnMut(web_sys::MediaQueryListEvent)>::new(move |event: web_sys::MediaQueryListEvent| {
            system_dark.set(event.matches());
        });
        query.set_onchange(Some(on_change.as_ref().unchecked_ref()));
        // Must outlive this function: the browser calls into it for the life of the page.
        on_change.forget();
    }

    Effect::new(move |_: Option<()>| {
        let attr = theme.get().resolve(system_dark.get()).attr_value();
        let element = web_sys::window().and_then(|w| w.document()).and_then(|d| d.document_element());
        if let Some(element) = element {
            if let Err(error) = element.set_attribute("data-theme", attr) {
                tracing::warn!(?error, "could not apply theme");
            }
        }
    });
}

#[derive(Clone, Copy)]
pub struct Prefs {
    pub teaching_mode: Persisted<bool>,
    pub theme: Persisted<Theme>,
    pub library: Persisted<Library>,
    pub player_name: Persisted<String>,
}

impl Prefs {
    pub fn load() -> Self {
        let prefs = Self {
            teaching_mode: Persisted::new("pref.teaching_mode", || true),
            theme: Persisted::new("pref.theme", Theme::default),
            library: Persisted::new("pref.saved_actions", Library::default),
            player_name: Persisted::new("pref.player_name", String::new),
        };
        install_theme(prefs.theme);
        prefs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_resolves_from_the_os_preference() {
        assert_eq!(Theme::System.resolve(true), Resolved::Dark);
        assert_eq!(Theme::System.resolve(false), Resolved::Light);
    }

    #[test]
    fn explicit_theme_ignores_the_os_preference() {
        assert_eq!(Theme::Light.resolve(true), Resolved::Light);
        assert_eq!(Theme::Dark.resolve(false), Resolved::Dark);
    }
}
