//! The raw websocket transport: opens a connection to the server's `/ws` endpoint, sends
//! `ClientEnvelope`s, and delivers `ServerEnvelope`s to a callback as they arrive. Knows nothing
//! about rooms, battles, permissions, or reconnection policy — see `battle_net.rs`, built on top
//! of this, for all of that.

mod error;

pub use error::SocketError;

use crate::config::ws_url;
use leptos::wasm_bindgen::closure::Closure;
use leptos::wasm_bindgen::{JsCast, JsValue};
use leptos::web_sys;
use shared::protocol::{ClientEnvelope, ServerEnvelope};
use std::cell::RefCell;
use std::rc::Rc;

/// A `DOMException` (the common failure shape from a JS API call) is not `instanceof Error`, so
/// wasm-bindgen's `Debug` for `JsValue` renders it as the bare word `DOMException` and drops the
/// message — same fix as `storage.rs`'s own `js_message`, kept separate here since neither module
/// depends on the other.
fn js_message(value: &JsValue) -> String {
    if let Some(exception) = value.dyn_ref::<web_sys::DomException>() {
        return format!("{}: {}", exception.name(), exception.message());
    }
    value.as_string().unwrap_or_else(|| format!("{value:?}"))
}

struct SocketInner {
    ws: web_sys::WebSocket,
    /// Anything sent before the socket actually reaches `OPEN` — `WebSocket::send` throws if
    /// called any earlier, and a fresh connection is never open synchronously, so every caller
    /// would otherwise need to hold its first message until it independently learned the socket
    /// was ready. Queueing here means `Socket::send` never fails for that reason: hand it a
    /// message the moment a connection is wanted, and it goes out exactly once, in order, the
    /// instant `onopen` fires.
    pending: Rc<RefCell<Vec<String>>>,
    // Held only to keep each closure alive for as long as the socket itself; never called.
    _on_open: Closure<dyn FnMut()>,
    _on_message: Closure<dyn FnMut(web_sys::MessageEvent)>,
    _on_close: Closure<dyn FnMut(web_sys::CloseEvent)>,
    _on_error: Closure<dyn FnMut(web_sys::Event)>,
}

#[derive(Clone)]
pub struct Socket(Rc<SocketInner>);

impl Socket {
    /// Opens a new connection immediately — there is no separate "wait until ready" step; see
    /// `send`'s doc comment on `pending`. `on_close` fires for a clean close, a network drop, and
    /// a failed connection attempt alike (a `WebSocket` that fails to open still transitions
    /// through `close`), so callers only ever need the one handler to detect "not connected
    /// anymore."
    pub fn connect(
        on_message: impl Fn(ServerEnvelope) + 'static,
        on_close: impl Fn() + 'static,
    ) -> Result<Self, SocketError> {
        let ws = web_sys::WebSocket::new(&ws_url()).map_err(|error| SocketError::Connect(js_message(&error)))?;
        let pending: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));

        let flush_ws = ws.clone();
        let flush_pending = pending.clone();
        let on_open = Closure::<dyn FnMut()>::new(move || {
            for json in flush_pending.borrow_mut().drain(..) {
                let _ = flush_ws.send_with_str(&json);
            }
        });
        ws.set_onopen(Some(on_open.as_ref().unchecked_ref()));

        let on_message = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |event: web_sys::MessageEvent| {
            let Some(text) = event.data().as_string() else { return };
            match shared::validate::decode::<ServerEnvelope>(&text) {
                Ok(envelope) => on_message(envelope),
                Err(error) => tracing::debug!(%error, "ignoring an undecodable server message"),
            }
        });
        ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));

        let on_close = Closure::<dyn FnMut(web_sys::CloseEvent)>::new(move |_event: web_sys::CloseEvent| on_close());
        ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));

        // A real failure always fires `close` right after -- this exists only to stop the
        // otherwise-uncaught JS exception from spamming the browser console; `on_close` above is
        // where actual cleanup and reconnection happen.
        let on_error = Closure::<dyn FnMut(web_sys::Event)>::new(|_event: web_sys::Event| {});
        ws.set_onerror(Some(on_error.as_ref().unchecked_ref()));

        Ok(Self(Rc::new(SocketInner { ws, pending, _on_open: on_open, _on_message: on_message, _on_close: on_close, _on_error: on_error })))
    }

    pub fn send(&self, envelope: &ClientEnvelope) -> Result<(), SocketError> {
        let json = serde_json::to_string(envelope).map_err(|error| SocketError::Encode(error.to_string()))?;
        if self.0.ws.ready_state() == web_sys::WebSocket::OPEN {
            self.0.ws.send_with_str(&json).map_err(|error| SocketError::Send(js_message(&error)))?;
        } else {
            self.0.pending.borrow_mut().push(json);
        }
        Ok(())
    }

    /// Idempotent-in-effect: closing an already-closing or already-closed socket is a no-op, not
    /// an error, per the `WebSocket` spec. Detaches every handler first, so a close this Battles
    /// initiates itself (e.g. `leave()`) never triggers the reconnect logic `Session` hangs on
    /// `on_close`.
    pub fn close(&self) {
        self.0.ws.set_onopen(None);
        self.0.ws.set_onmessage(None);
        self.0.ws.set_onclose(None);
        self.0.ws.set_onerror(None);
        let _ = self.0.ws.close();
    }
}
