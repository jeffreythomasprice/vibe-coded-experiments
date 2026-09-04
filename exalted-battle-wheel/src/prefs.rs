//! Generic, typed preferences backed by `storage.rs`. `Pref<T>` works for any
//! `Serialize + DeserializeOwned` type — bool, enum, struct — not just the two prefs the app
//! currently has. Add a preference by adding one field to `Prefs` and one line to `Prefs::load`.

use crate::storage::{self, StorageError};
use leptos::prelude::*;
use leptos::wasm_bindgen::closure::Closure;
use leptos::wasm_bindgen::JsCast;
use leptos::web_sys;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::ops::Deref;

const KEY_PREFIX: &str = "ebw.pref.";

#[derive(Debug, thiserror::Error)]
pub enum PrefError {
    #[error("could not encode preference {key:?}: {source}")]
    Encode { key: &'static str, #[source] source: serde_json::Error },
    #[error("could not decode preference {key:?}: {source}")]
    Decode { key: &'static str, #[source] source: serde_json::Error },
    #[error(transparent)]
    Storage(#[from] StorageError),
}

fn storage_key(key: &str) -> String {
    format!("{KEY_PREFIX}{key}")
}

fn encode<T: Serialize>(key: &'static str, value: &T) -> Result<String, PrefError> {
    serde_json::to_string(value).map_err(|source| PrefError::Encode { key, source })
}

fn decode<T: DeserializeOwned>(key: &'static str, json: &str) -> Result<T, PrefError> {
    serde_json::from_str(json).map_err(|source| PrefError::Decode { key, source })
}

fn load<T: DeserializeOwned>(key: &'static str) -> Result<Option<T>, PrefError> {
    match storage::get(&storage_key(key))? {
        Some(json) => decode(key, &json).map(Some),
        None => Ok(None),
    }
}

/// Stores only non-default values, so a later change to a default still reaches everyone who
/// never touched the setting, and `reset()` (a plain `value.set(default)`) naturally cleans the
/// key back up the next time this runs rather than racing it.
fn store(key: &'static str, json: &str, default_json: &str) -> Result<(), PrefError> {
    let full_key = storage_key(key);
    if json == default_json {
        storage::remove(&full_key)?;
    } else {
        storage::set(&full_key, json)?;
    }
    Ok(())
}

/// A persisted, reactive preference. `Deref`s to its `RwSignal<T>` so `.get()`, `.set()`, and
/// `.get_untracked()` work exactly like an ordinary signal at every call site.
pub struct Pref<T: 'static> {
    default: fn() -> T,
    value: RwSignal<T>,
}

impl<T> Clone for Pref<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Pref<T> {}

impl<T> Deref for Pref<T> {
    type Target = RwSignal<T>;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T> Pref<T>
where
    T: Serialize + DeserializeOwned + PartialEq + Clone + Send + Sync + 'static,
{
    pub fn new(key: &'static str, default: fn() -> T) -> Self {
        let fallback = default();
        let default_json = encode(key, &fallback).unwrap_or_default();

        let initial = match load(key) {
            Ok(Some(stored)) => stored,
            Ok(None) => fallback,
            Err(error) => {
                tracing::warn!(%error, "using the default preference");
                fallback
            }
        };

        let value = RwSignal::new(initial);

        Effect::new(move |previous: Option<()>| {
            let current = value.read();
            if previous.is_none() {
                return;
            }
            let result = encode(key, &*current).and_then(|json| store(key, &json, &default_json));
            if let Err(error) = result {
                tracing::warn!(%error, "could not save preference");
            }
        });

        let full_key = storage_key(key);
        window_event_listener(leptos::ev::storage, move |event: web_sys::StorageEvent| {
            if event.key().as_deref() != Some(full_key.as_str()) {
                return;
            }
            let incoming = match event.new_value() {
                Some(raw) => match decode::<T>(key, &raw) {
                    Ok(decoded) => decoded,
                    Err(error) => {
                        tracing::warn!(%error, "ignoring invalid cross-tab preference update");
                        return;
                    }
                },
                None => default(),
            };
            if value.get_untracked() != incoming {
                value.set(incoming);
            }
        });

        Self { default, value }
    }

    pub fn reset(&self) {
        self.value.set((self.default)());
    }
}

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
fn install_theme(theme: Pref<Theme>) {
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
    pub teaching_mode: Pref<bool>,
    pub theme: Pref<Theme>,
}

impl Prefs {
    pub fn load() -> Self {
        let prefs = Self { teaching_mode: Pref::new("teaching_mode", || true), theme: Pref::new("theme", Theme::default) };
        install_theme(prefs.theme);
        prefs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespaces_keys() {
        assert_eq!(storage_key("teaching_mode"), "ebw.pref.teaching_mode");
    }

    #[test]
    fn round_trips_a_bool() {
        let json = encode("teaching_mode", &false).unwrap();
        assert_eq!(json, "false");
        assert!(!decode::<bool>("teaching_mode", &json).unwrap());
    }

    #[test]
    fn round_trips_an_enum() {
        let json = encode("theme", &Theme::Dark).unwrap();
        assert_eq!(json, "\"dark\"");
        assert_eq!(decode::<Theme>("theme", &json).unwrap(), Theme::Dark);
    }

    #[test]
    fn rejects_a_corrupt_value() {
        let error = decode::<bool>("teaching_mode", "not json").unwrap_err();
        assert!(matches!(error, PrefError::Decode { .. }));
    }

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
