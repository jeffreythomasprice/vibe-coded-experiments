//! Generic, typed, local-storage-backed reactive state. `Persisted<T>` works for any
//! `Serialize + DeserializeOwned` type — bool, enum, struct, event log — not just preferences.
//! `prefs.rs` builds `Prefs` on top of this; `app.rs` uses it directly for the battle log.

use crate::storage::{self, StorageError};
use leptos::prelude::*;
use leptos::web_sys;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::ops::Deref;

const KEY_PREFIX: &str = "ebw.";

#[derive(Debug, thiserror::Error)]
pub enum PersistError {
    #[error("could not encode {key:?}: {source}")]
    Encode { key: &'static str, #[source] source: serde_json::Error },
    #[error("could not decode {key:?}: {source}")]
    Decode { key: &'static str, #[source] source: serde_json::Error },
    #[error(transparent)]
    Storage(#[from] StorageError),
}

fn storage_key(key: &str) -> String {
    format!("{KEY_PREFIX}{key}")
}

fn encode<T: Serialize>(key: &'static str, value: &T) -> Result<String, PersistError> {
    serde_json::to_string(value).map_err(|source| PersistError::Encode { key, source })
}

fn decode<T: DeserializeOwned>(key: &'static str, json: &str) -> Result<T, PersistError> {
    serde_json::from_str(json).map_err(|source| PersistError::Decode { key, source })
}

fn load<T: DeserializeOwned>(key: &'static str) -> Result<Option<T>, PersistError> {
    match storage::get(&storage_key(key))? {
        Some(json) => decode(key, &json).map(Some),
        None => Ok(None),
    }
}

/// Stores only non-default values, so a later change to a default still reaches everyone who
/// never touched the value, and setting the value back to its default naturally cleans the key
/// back up the next time this runs rather than needing a separate "was this ever touched" flag.
fn store(key: &'static str, json: &str, default_json: &str) -> Result<(), PersistError> {
    let full_key = storage_key(key);
    if json == default_json {
        storage::remove(&full_key)?;
    } else {
        storage::set(&full_key, json)?;
    }
    Ok(())
}

/// A value backed by local storage: reactive like a plain signal, but loaded once at startup and
/// autosaved on every change. `Deref`s to its `RwSignal<T>` so `.get()`, `.set()`, and
/// `.get_untracked()` work exactly like an ordinary signal at every call site.
pub struct Persisted<T: 'static> {
    value: RwSignal<T>,
}

impl<T> Clone for Persisted<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Persisted<T> {}

impl<T> Deref for Persisted<T> {
    type Target = RwSignal<T>;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T> Persisted<T>
where
    T: Serialize + DeserializeOwned + PartialEq + Clone + Send + Sync + 'static,
{
    /// Loads `key` (or falls back to `default()`), and starts autosaving and cross-tab syncing.
    /// A load or save failure is always logged at `error!` (the release build's tracing filter
    /// drops `warn!` and below — see `main.rs`) and reported to the toast layer if one is
    /// mounted; a load failure also removes the unreadable key so it doesn't fail again next
    /// time.
    pub fn new(key: &'static str, default: fn() -> T) -> Self {
        Self::new_gated(key, default, || true)
    }

    /// Like `new`, but `accept_remote` is checked before ever applying an update that arrived via
    /// another tab's `storage` event. Only the battle log needs this: two tabs sharing
    /// `localStorage` is exactly the existing (desired) cross-tab sync in `Mode::Solo`, but once
    /// this tab has adopted a room's battle, an unrelated edit in a plain Solo tab must not
    /// silently overwrite it — `accept_remote` is the negation of whatever signal `Session`
    /// flips while a room is active (see `app.rs`, which wires the two together).
    pub fn new_gated(key: &'static str, default: fn() -> T, accept_remote: impl Fn() -> bool + 'static) -> Self {
        let fallback = default();
        let default_json = encode(key, &fallback).unwrap_or_default();

        let initial = match load(key) {
            Ok(Some(stored)) => stored,
            Ok(None) => fallback,
            Err(error) => {
                tracing::error!(%error, "could not load saved state; starting fresh");
                if let Err(remove_error) = storage::remove(&storage_key(key)) {
                    tracing::error!(%remove_error, "could not clear unreadable saved state");
                }
                crate::ui::toast::error(format!("Could not load saved data: {error}"));
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
                tracing::error!(%error, "could not save state");
                crate::ui::toast::error(format!("Could not save: {error}"));
            }
        });

        let full_key = storage_key(key);
        window_event_listener(leptos::ev::storage, move |event: web_sys::StorageEvent| {
            if event.key().as_deref() != Some(full_key.as_str()) {
                return;
            }
            if !accept_remote() {
                return;
            }
            let incoming = match event.new_value() {
                Some(raw) => match decode::<T>(key, &raw) {
                    Ok(decoded) => decoded,
                    Err(error) => {
                        tracing::error!(%error, "ignoring invalid cross-tab update");
                        return;
                    }
                },
                None => default(),
            };
            if value.get_untracked() != incoming {
                value.set(incoming);
            }
        });

        Self { value }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "kebab-case")]
    enum Setting {
        Dark,
    }

    #[test]
    fn namespaces_keys() {
        assert_eq!(storage_key("pref.teaching_mode"), "ebw.pref.teaching_mode");
    }

    #[test]
    fn battle_state_sits_outside_the_preference_namespace() {
        assert_eq!(storage_key("battle"), "ebw.battle");
    }

    #[test]
    fn round_trips_a_bool() {
        let json = encode("pref.teaching_mode", &false).unwrap();
        assert_eq!(json, "false");
        assert!(!decode::<bool>("pref.teaching_mode", &json).unwrap());
    }

    #[test]
    fn round_trips_an_enum() {
        let json = encode("pref.theme", &Setting::Dark).unwrap();
        assert_eq!(json, "\"dark\"");
        assert_eq!(decode::<Setting>("pref.theme", &json).unwrap(), Setting::Dark);
    }

    #[test]
    fn rejects_a_corrupt_value() {
        let error = decode::<bool>("pref.teaching_mode", "not json").unwrap_err();
        assert!(matches!(error, PersistError::Decode { .. }));
    }
}
