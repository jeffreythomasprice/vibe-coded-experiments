//! The one place that touches the async clipboard API. Shaped like `toast.rs`: a free function any
//! caller can reach, plus the one component that actually uses it.

use crate::ui::glossary::Topic;
use crate::ui::Tip;
use js_sys::{Array, Promise};
use leptos::prelude::*;
use leptos::wasm_bindgen::closure::Closure;
use leptos::wasm_bindgen::{JsCast, JsValue};
use leptos::web_sys;
use std::time::Duration;
use wasm_bindgen_futures::JsFuture;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClipboardError {
    /// `navigator.clipboard` only exists in a secure context (https, or localhost) — served any
    /// other way, the property is simply absent rather than throwing.
    #[error("this browser will not give this page clipboard access")]
    Unavailable,
    /// A permissions policy, a denied prompt, or a write attempted outside a user gesture.
    #[error("the browser refused to write to the clipboard: {0}")]
    Refused(String),
    /// `writeText` is documented to reject promptly when the browser refuses, but some
    /// environments (confirmed: a remotely-driven Chrome tab with no real clipboard backend) leave
    /// the promise neither resolved nor rejected instead. Racing a timeout is what turns that into
    /// an answer at all, rather than a "Copy" button stuck silently forever.
    #[error("copying timed out")]
    TimedOut,
}

/// A `DOMException` (the common failure shape here) is not `instanceof Error`, so wasm-bindgen's
/// `Debug` for `JsValue` renders it as the bare word `DOMException` and drops the message. Read its
/// fields directly instead — same fix as `storage.rs`'s and `net::rtc`'s `js_message`.
fn js_message(error: &JsValue) -> String {
    if let Some(exception) = error.dyn_ref::<web_sys::DomException>() {
        return format!("{}: {}", exception.name(), exception.message());
    }
    error.as_string().unwrap_or_else(|| format!("{error:?}"))
}

/// Reached through `Reflect` rather than `Navigator::clipboard()` directly: where the API is
/// absent, the property is `undefined`, and wasm-bindgen's own getter throws in that case instead
/// of yielding an `Option` — so probing this way is what turns a missing API into a `None` instead
/// of a panic.
fn clipboard() -> Option<web_sys::Clipboard> {
    let navigator = web_sys::window()?.navigator();
    let value = js_sys::Reflect::get(&navigator, &JsValue::from_str("clipboard")).ok()?;
    (!value.is_undefined() && !value.is_null()).then(|| value.unchecked_into::<web_sys::Clipboard>())
}

/// How long to wait for `writeText` before giving up on it — see `ClipboardError::TimedOut`.
const CLIPBOARD_TIMEOUT_MS: i32 = 5000;

/// Distinguishes the timeout's own rejection from a genuine `writeText` rejection, both of which
/// arrive at the same `map_err` below.
const TIMEOUT_SENTINEL: &str = "exalted-battle-wheel::clipboard-write-timeout";

/// Rejects with `TIMEOUT_SENTINEL` after `ms` — shaped like `net::rtc`'s `sleep_ms`, but rejecting
/// instead of resolving so `Promise::race` can tell "the write never finished" apart from "it
/// finished, successfully or not" without polling.
fn timeout_promise(ms: i32) -> Promise {
    Promise::new(&mut |_resolve, reject| {
        if let Some(window) = web_sys::window() {
            let on_timeout = Closure::<dyn FnMut()>::new(move || {
                let _ = reject.call1(&JsValue::NULL, &JsValue::from_str(TIMEOUT_SENTINEL));
            });
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(on_timeout.as_ref().unchecked_ref(), ms);
            on_timeout.forget();
        }
    })
}

pub async fn write_text(text: &str) -> Result<(), ClipboardError> {
    let clipboard = clipboard().ok_or(ClipboardError::Unavailable)?;
    let race = Promise::race(&Array::of2(&clipboard.write_text(text), &timeout_promise(CLIPBOARD_TIMEOUT_MS)));
    JsFuture::from(race).await.map(|_| ()).map_err(|error| {
        if error.as_string().as_deref() == Some(TIMEOUT_SENTINEL) {
            ClipboardError::TimedOut
        } else {
            ClipboardError::Refused(js_message(&error))
        }
    })
}

/// Reverts rather than latching, so the label is feedback about the click that just happened, not
/// a standing claim about what's on the clipboard now.
const COPIED_LABEL_MS: u64 = 1500;

/// Copies `text` and swaps its own label to "Copied!" for a moment. `text` is a plain `String`
/// rather than a signal: the caller's `move ||` already rebuilds this component whenever the code
/// it copies changes (the host re-mints the invite on every join), which is exactly when the old
/// "Copied!" state should reset anyway.
#[component]
pub fn CopyButton(text: String) -> impl IntoView {
    let copied = RwSignal::new(false);
    let copy = move |_| {
        let text = text.clone();
        leptos::task::spawn_local_scoped(async move {
            match write_text(&text).await {
                Ok(()) => {
                    copied.set(true);
                    set_timeout(move || copied.set(false), Duration::from_millis(COPIED_LABEL_MS));
                }
                Err(error) => {
                    tracing::error!(%error, "could not copy to the clipboard");
                    crate::ui::toast::error(format!("Could not copy: {error}"));
                }
            }
        });
    };

    view! {
        <Tip topic=Topic::RoomCopyCode>
            <button class="btn room-copy" on:click=copy>
                {move || if copied.get() { "Copied!" } else { "Copy" }}
            </button>
        </Tip>
    }
}
