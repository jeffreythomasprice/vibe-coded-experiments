//! Thin, string-only wrapper over `window().local_storage()`. Knows nothing about preferences —
//! `prefs.rs` builds the typed, reactive layer on top of this.

use leptos::wasm_bindgen::{JsCast, JsValue};
use leptos::web_sys;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StorageError {
    #[error("there is no window object")]
    NoWindow,
    #[error("local storage is unavailable: {0}")]
    Unavailable(String),
    #[error("local storage is disabled")]
    Disabled,
    #[error("could not read {key:?}: {message}")]
    Read { key: String, message: String },
    #[error("could not write {key:?}: {message}")]
    Write { key: String, message: String },
    #[error("could not remove {key:?}: {message}")]
    Remove { key: String, message: String },
}

/// A `DOMException` (the usual failure here — quota exceeded, storage disabled in a private
/// window) is not `instanceof Error`, so wasm-bindgen's `Debug` for `JsValue` renders it as the
/// bare word `DOMException` and drops the message entirely. Read its fields directly instead.
fn js_message(error: &JsValue) -> String {
    if let Some(exception) = error.dyn_ref::<web_sys::DomException>() {
        return format!("{}: {}", exception.name(), exception.message());
    }
    error.as_string().unwrap_or_else(|| format!("{error:?}"))
}

fn local_storage() -> Result<web_sys::Storage, StorageError> {
    web_sys::window()
        .ok_or(StorageError::NoWindow)?
        .local_storage()
        .map_err(|error| StorageError::Unavailable(js_message(&error)))?
        .ok_or(StorageError::Disabled)
}

pub fn get(key: &str) -> Result<Option<String>, StorageError> {
    local_storage()?
        .get_item(key)
        .map_err(|error| StorageError::Read { key: key.to_owned(), message: js_message(&error) })
}

pub fn set(key: &str, value: &str) -> Result<(), StorageError> {
    local_storage()?
        .set_item(key, value)
        .map_err(|error| StorageError::Write { key: key.to_owned(), message: js_message(&error) })
}

pub fn remove(key: &str) -> Result<(), StorageError> {
    local_storage()?
        .remove_item(key)
        .map_err(|error| StorageError::Remove { key: key.to_owned(), message: js_message(&error) })
}
