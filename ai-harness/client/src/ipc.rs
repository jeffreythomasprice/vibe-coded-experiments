//! The bridge to the Tauri backend.
//!
//! `withGlobalTauri` is enabled in `tauri.conf.json`, so both `invoke` and
//! `Channel` are reachable at `window.__TAURI__.core.*` without a JS package.

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_wasm_bindgen::{from_value, to_value};
use shared::error::{ErrorKind, ErrorReport};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

#[wasm_bindgen]
extern "C" {
    /// `catch` because a command that returns `Result::Err` rejects the JS
    /// promise — without it, that rejection would trap the wasm future
    /// instead of coming back as an `Err` here.
    #[wasm_bindgen(catch, js_namespace = ["window", "__TAURI__", "core"], js_name = invoke)]
    pub(crate) async fn invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;

    /// A live channel a streaming command's `on_event` argument turns into a
    /// series of `onmessage` calls. Constructed fresh per call — it can't
    /// cross `serde_wasm_bindgen` like a plain value, since it's a stateful
    /// JS object, not data.
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], js_name = Channel)]
    type Channel;

    #[wasm_bindgen(constructor, js_namespace = ["window", "__TAURI__", "core"])]
    fn new() -> Channel;

    #[wasm_bindgen(method, setter, js_name = onmessage)]
    fn set_onmessage(this: &Channel, cb: &JsValue);
}

/// Call a Tauri command with typed args and a typed return.
///
/// `args` must serialize to a plain object whose field names already match
/// what the command expects — Tauri v2 camel-cases a command's parameter
/// names on the JS side (`conversation_id` -> `conversationId`), so every
/// args struct passed here needs `#[serde(rename_all = "camelCase")]`.
///
/// A rejected call is translated to [`ErrorReport`]: every command in
/// `server::commands` already rejects with that shape as JSON, so decoding
/// it back is the common case. `model_catalog` is the one command that still
/// rejects with a bare `String` — that (and any other shape mismatch) falls
/// back to `ErrorKind::Other` carrying the raw text, so no caller ever sees a
/// silent panic on a rejection this crate didn't anticipate.
pub async fn call<A: Serialize, T: DeserializeOwned>(cmd: &str, args: &A) -> Result<T, ErrorReport> {
    let args_js = to_value(args).map_err(|err| ErrorReport {
        kind: ErrorKind::Other,
        provider: None,
        message: format!("failed to serialize arguments for {cmd}: {err}"),
        retryable: false,
    })?;
    let result = invoke(cmd, args_js).await.map_err(|err| decode_rejection(cmd, err))?;
    decode_result(cmd, result)
}

/// Call a Tauri command that takes no arguments (e.g. `list_agents`,
/// `model_catalog`) — `JsValue::UNDEFINED`, not a serialized `()`, is what
/// `invoke` expects here.
pub async fn call0<T: DeserializeOwned>(cmd: &str) -> Result<T, ErrorReport> {
    let result = invoke(cmd, JsValue::UNDEFINED).await.map_err(|err| decode_rejection(cmd, err))?;
    decode_result(cmd, result)
}

/// Call a Tauri command that streams events over an `on_event` channel
/// before resolving to its final, typed result.
///
/// `args` serializes the same way [`call`]'s does; the channel is added
/// alongside it under `onEvent`, which is where Tauri expects a streaming
/// command's channel argument by its camelCased Rust name. `on_event` is
/// invoked, on the browser's microtask queue, once per event the backend
/// sends — an event this crate fails to decode is logged and dropped rather
/// than failing the whole call, since one malformed event shouldn't hide
/// every other message already in flight.
///
/// The `Closure` backing the channel is dropped only after `invoke` resolves
/// — it must outlive every `.await` point where the backend could still call
/// it, but there is no reason to `.forget()` (and leak) it beyond that.
pub async fn call_streaming<A, E, T>(
    cmd: &str,
    args: &A,
    mut on_event: impl FnMut(E) + 'static,
) -> Result<T, ErrorReport>
where
    A: Serialize,
    E: DeserializeOwned + 'static,
    T: DeserializeOwned,
{
    let args_js = to_value(args).map_err(|err| ErrorReport {
        kind: ErrorKind::Other,
        provider: None,
        message: format!("failed to serialize arguments for {cmd}: {err}"),
        retryable: false,
    })?;

    let channel = Channel::new();
    let closure = Closure::wrap(Box::new(move |event: JsValue| match from_value::<E>(event) {
        Ok(event) => on_event(event),
        Err(err) => tracing::warn!(%err, cmd, "dropping an undecodable streamed event"),
    }) as Box<dyn FnMut(JsValue)>);
    channel.set_onmessage(closure.as_ref().unchecked_ref());

    js_sys::Reflect::set(&args_js, &JsValue::from_str("onEvent"), &channel)
        .expect("Reflect::set on a freshly serialized plain object cannot fail");

    let result = invoke(cmd, args_js).await.map_err(|err| decode_rejection(cmd, err));
    drop(closure);
    decode_result(cmd, result?)
}

fn decode_rejection(cmd: &str, err: JsValue) -> ErrorReport {
    if let Ok(report) = from_value::<ErrorReport>(err.clone()) {
        return report;
    }
    let message = err.as_string().unwrap_or_else(|| format!("{err:?}"));
    ErrorReport {
        kind: ErrorKind::Other,
        provider: None,
        message: format!("{cmd}: {message}"),
        retryable: false,
    }
}

fn decode_result<T: DeserializeOwned>(cmd: &str, value: JsValue) -> Result<T, ErrorReport> {
    from_value(value).map_err(|err| ErrorReport {
        kind: ErrorKind::Other,
        provider: None,
        message: format!("failed to parse response from {cmd}: {err}"),
        retryable: false,
    })
}
