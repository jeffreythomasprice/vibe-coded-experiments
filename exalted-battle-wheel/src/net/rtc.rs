//! The only file in this crate that touches raw WebRTC. `Link` hides the promise-and-closure
//! plumbing behind two async constructors, `accept_answer`, and `send`; callers never see a
//! `JsValue`. No Leptos dependency here on purpose — connection status and message routing are
//! delivered through plain callbacks, not signals, so this stays usable from anywhere (a `Session`
//! method, not just component code) without needing a reactive owner to exist.

use crate::net::code;
use crate::net::error::{RoomError, RtcError};
use js_sys::{Array, Promise};
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    MessageEvent, RtcConfiguration, RtcDataChannel, RtcDataChannelEvent, RtcIceGatheringState, RtcIceServer,
    RtcPeerConnection, RtcSdpType, RtcSessionDescriptionInit,
};

const DEFAULT_STUN_SERVERS: &[&str] = &["stun:stun.l.google.com:19302", "stun:stun.cloudflare.com:3478"];

thread_local! {
    /// The STUN server list every new connection is configured with. Seeded from
    /// `DEFAULT_STUN_SERVERS`, then overwritable at any time — from a `#stun=` URL fragment at
    /// boot (see `app.rs`) or live from the room modal's Advanced section. A `RefCell`, not a
    /// Leptos signal: this file deliberately has no Leptos dependency, and nothing outside the
    /// modal needs to react to a change — only the next connection attempt reads it.
    static STUN_SERVERS: RefCell<Vec<String>> = RefCell::new(default_stun_servers());
}

pub fn default_stun_servers() -> Vec<String> {
    DEFAULT_STUN_SERVERS.iter().map(|url| url.to_string()).collect()
}

pub fn stun_servers() -> Vec<String> {
    STUN_SERVERS.with_borrow(Clone::clone)
}

/// Overrides the STUN server list for every connection from here on. Read fresh by
/// `new_peer_connection` on each `host()`/`join()`, so a change reaches the next connection
/// attempt with no further wiring.
pub fn set_stun_servers(servers: Vec<String>) {
    STUN_SERVERS.set(servers);
}

/// How long to wait for ICE candidate gathering before proceeding with whatever was found. srflx
/// candidates normally arrive in 50-300ms and gathering ends early once they do; this ceiling
/// only costs anything on a genuinely slow network.
const ICE_GATHERING_TIMEOUT_MS: i32 = 3000;

/// Resolves after `ms` milliseconds — used by `Session` to time out a two-phase-commit vote that
/// a peer never answers. A macrotask (`setTimeout`), not a microtask: this genuinely needs to wait
/// for real elapsed time, unlike the reentrancy-avoiding `spawn_local` deferrals elsewhere, which
/// only need to yield past the current call stack.
pub async fn sleep_ms(ms: i32) {
    let promise = Promise::new(&mut |resolve, _reject| {
        if let Some(window) = web_sys::window() {
            let on_timeout = Closure::<dyn FnMut()>::new(move || {
                let _ = resolve.call0(&JsValue::NULL);
            });
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(on_timeout.as_ref().unchecked_ref(), ms);
            on_timeout.forget();
        }
    });
    let _ = JsFuture::from(promise).await;
}

/// A `DOMException` (the common failure shape here) is not `instanceof Error`, so
/// wasm-bindgen's `Debug` for `JsValue` renders it as the bare word `DOMException` and drops the
/// message. Read its fields directly instead — same fix as `storage.rs`'s `js_message`.
fn js_message(error: &JsValue) -> String {
    if let Some(exception) = error.dyn_ref::<web_sys::DomException>() {
        return format!("{}: {}", exception.name(), exception.message());
    }
    error.as_string().unwrap_or_else(|| format!("{error:?}"))
}

fn new_peer_connection() -> Result<RtcPeerConnection, RtcError> {
    let servers = Array::new();
    for url in stun_servers() {
        let server = RtcIceServer::new();
        server.set_urls_str(&url);
        servers.push(&server);
    }
    let config = RtcConfiguration::new();
    config.set_ice_servers(&servers);
    RtcPeerConnection::new_with_configuration(&config).map_err(|error| RtcError::PeerConnection(js_message(&error)))
}

async fn negotiate_local(pc: &RtcPeerConnection, kind: &'static str) -> Result<(), RtcError> {
    let promise = if kind == "offer" { pc.create_offer() } else { pc.create_answer() };
    let description = JsFuture::from(promise).await.map_err(|error| RtcError::Negotiate { kind, message: js_message(&error) })?;
    let init: RtcSessionDescriptionInit = description.unchecked_into();
    JsFuture::from(pc.set_local_description(&init))
        .await
        .map_err(|error| RtcError::Description { which: "local", message: js_message(&error) })?;
    Ok(())
}

async fn set_remote(pc: &RtcPeerConnection, kind: RtcSdpType, sdp: &str) -> Result<(), RtcError> {
    let init = RtcSessionDescriptionInit::new(kind);
    init.set_sdp(sdp);
    JsFuture::from(pc.set_remote_description(&init))
        .await
        .map_err(|error| RtcError::Description { which: "remote", message: js_message(&error) })?;
    Ok(())
}

/// Waits for ICE gathering to finish, or gives up after `ICE_GATHERING_TIMEOUT_MS` and proceeds
/// with whatever candidates were found by then — there is no signaling channel to trickle late
/// candidates over, so every candidate that matters has to be baked into the description we hand
/// back at the end of this wait.
async fn wait_for_ice_gathering(pc: &RtcPeerConnection) {
    if pc.ice_gathering_state() == RtcIceGatheringState::Complete {
        return;
    }
    let promise = Promise::new(&mut |resolve, _reject| {
        let watched = pc.clone();
        let on_change_resolve = resolve.clone();
        let on_change = Closure::<dyn FnMut()>::new(move || {
            if watched.ice_gathering_state() == RtcIceGatheringState::Complete {
                let _ = on_change_resolve.call0(&JsValue::NULL);
            }
        });
        pc.set_onicegatheringstatechange(Some(on_change.as_ref().unchecked_ref()));
        // One-shot for this connection's setup phase only, not a per-message handler — bounded to
        // one leaked closure per connection attempt, same tradeoff `prefs.rs` makes for its
        // page-lifetime media-query listener.
        on_change.forget();

        if let Some(window) = web_sys::window() {
            let on_timeout = Closure::<dyn FnMut()>::new(move || {
                let _ = resolve.call0(&JsValue::NULL);
            });
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(on_timeout.as_ref().unchecked_ref(), ICE_GATHERING_TIMEOUT_MS);
            on_timeout.forget();
        }
    });
    let _ = JsFuture::from(promise).await;
}

fn local_sdp(pc: &RtcPeerConnection) -> Result<String, RtcError> {
    pc.local_description()
        .map(|description| description.sdp())
        .ok_or_else(|| RtcError::Description { which: "local", message: "no local description was set".to_string() })
}

struct LinkInner {
    pc: RtcPeerConnection,
    channel: RefCell<Option<RtcDataChannel>>,
    // Boxed once at construction and read whenever a channel event needs to fire one — this is
    // what lets `on_open`/`on_message` hand back a usable `Link` even on the hosting side, where
    // the `Link` value itself doesn't exist yet at the point these need to be attached to the
    // channel (created synchronously; `Link` is only returned once negotiation finishes).
    on_open_cb: Box<dyn Fn(Link)>,
    on_message_cb: Box<dyn Fn(Link, String)>,
    on_close_cb: Box<dyn Fn()>,
    // Owned so they're dropped (and detached) once nothing references this link; `on_data_channel`
    // is only ever set on the joining side, since the hosting side already holds its channel.
    on_data_channel: RefCell<Option<Closure<dyn FnMut(RtcDataChannelEvent)>>>,
    on_open: RefCell<Option<Closure<dyn FnMut()>>>,
    on_message: RefCell<Option<Closure<dyn FnMut(MessageEvent)>>>,
    on_close: RefCell<Option<Closure<dyn FnMut()>>>,
}

/// One WebRTC peer connection plus its one data channel. Cloning shares the same underlying
/// connection (an `Rc`) rather than creating a second one.
#[derive(Clone)]
pub struct Link(Rc<LinkInner>);

/// Attaches open/message/close handlers to `channel` and stores them on `inner`. Used identically
/// whether the channel was created locally (hosting) or arrived via `ondatachannel` (joining) —
/// weak references back into `inner` so a channel event never keeps a closed-over connection alive.
fn attach_channel_handlers(inner: &Rc<LinkInner>, channel: &RtcDataChannel) {
    let weak = Rc::downgrade(inner);
    let on_open = Closure::<dyn FnMut()>::new(move || {
        if let Some(inner) = weak.upgrade() {
            (inner.on_open_cb)(Link(inner.clone()));
        }
    });
    channel.set_onopen(Some(on_open.as_ref().unchecked_ref()));

    let weak = Rc::downgrade(inner);
    let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
        let Some(inner) = weak.upgrade() else { return };
        if let Some(text) = event.data().as_string() {
            (inner.on_message_cb)(Link(inner.clone()), text);
        }
    });
    channel.set_onmessage(Some(on_message.as_ref().unchecked_ref()));

    let weak = Rc::downgrade(inner);
    let on_close = Closure::<dyn FnMut()>::new(move || {
        if let Some(inner) = weak.upgrade() {
            (inner.on_close_cb)();
        }
    });
    channel.set_onclose(Some(on_close.as_ref().unchecked_ref()));

    *inner.channel.borrow_mut() = Some(channel.clone());
    *inner.on_open.borrow_mut() = Some(on_open);
    *inner.on_message.borrow_mut() = Some(on_message);
    *inner.on_close.borrow_mut() = Some(on_close);
}

fn new_inner(
    pc: RtcPeerConnection,
    on_open: impl Fn(Link) + 'static,
    on_message: impl Fn(Link, String) + 'static,
    on_close: impl Fn() + 'static,
) -> Rc<LinkInner> {
    Rc::new(LinkInner {
        pc,
        channel: RefCell::new(None),
        on_open_cb: Box::new(on_open),
        on_message_cb: Box::new(on_message),
        on_close_cb: Box::new(on_close),
        on_data_channel: RefCell::new(None),
        on_open: RefCell::new(None),
        on_message: RefCell::new(None),
        on_close: RefCell::new(None),
    })
}

impl Link {
    /// Starts hosting: creates the data channel, produces an offer, and waits out ICE gathering.
    /// Returns the link (not yet connected — nobody has answered) and the encoded offer to hand
    /// to a joiner out of band. `on_open`/`on_message`/`on_close` fire for this one channel only.
    pub async fn host(
        on_open: impl Fn(Link) + 'static,
        on_message: impl Fn(Link, String) + 'static,
        on_close: impl Fn() + 'static,
    ) -> Result<(Self, String), RoomError> {
        let pc = new_peer_connection()?;
        let channel = pc.create_data_channel("battle");
        let inner = new_inner(pc, on_open, on_message, on_close);
        attach_channel_handlers(&inner, &channel);

        negotiate_local(&inner.pc, "offer").await?;
        wait_for_ice_gathering(&inner.pc).await;
        let sdp = local_sdp(&inner.pc)?;
        Ok((Self(inner), code::encode(&sdp)))
    }

    /// Completes hosting once the joiner has pasted back their answer.
    pub async fn accept_answer(&self, code: &str) -> Result<(), RoomError> {
        let sdp = code::decode(code)?;
        set_remote(&self.0.pc, RtcSdpType::Answer, &sdp).await?;
        Ok(())
    }

    /// Joins a hosted room from its encoded offer: sets it as the remote description, produces an
    /// answer, and waits out ICE gathering. Returns the link and the encoded answer to hand back
    /// to the host.
    pub async fn join(
        offer_code: &str,
        on_open: impl Fn(Link) + 'static,
        on_message: impl Fn(Link, String) + 'static,
        on_close: impl Fn() + 'static,
    ) -> Result<(Self, String), RoomError> {
        let offer_sdp = code::decode(offer_code)?;
        let pc = new_peer_connection()?;
        let inner = new_inner(pc, on_open, on_message, on_close);

        let weak = Rc::downgrade(&inner);
        let on_data_channel = Closure::<dyn FnMut(RtcDataChannelEvent)>::new(move |event: RtcDataChannelEvent| {
            let Some(inner) = weak.upgrade() else { return };
            attach_channel_handlers(&inner, &event.channel());
        });
        inner.pc.set_ondatachannel(Some(on_data_channel.as_ref().unchecked_ref()));
        *inner.on_data_channel.borrow_mut() = Some(on_data_channel);

        set_remote(&inner.pc, RtcSdpType::Offer, &offer_sdp).await?;
        negotiate_local(&inner.pc, "answer").await?;
        wait_for_ice_gathering(&inner.pc).await;
        let sdp = local_sdp(&inner.pc)?;
        Ok((Self(inner), code::encode(&sdp)))
    }

    pub fn send(&self, text: &str) -> Result<(), RtcError> {
        let channel = self.0.channel.borrow();
        let channel = channel.as_ref().ok_or(RtcError::ChannelClosed)?;
        channel.send_with_str(text).map_err(|error| RtcError::Send(js_message(&error)))
    }

    /// Closes the connection deliberately (a kick or a voluntary leave). Detaches every handler
    /// *before* closing, not after: closing fires `onclose` asynchronously, and this `Link` (the
    /// last strong reference to `LinkInner`, once its owner drops its own copy right after calling
    /// this) can easily be gone by the time that event actually arrives. Invoking a `Closure`
    /// after it's been dropped is a hard wasm-bindgen panic, not a silent no-op — detaching first
    /// means there is nothing left for that late event to call.
    pub fn close(&self) {
        if let Some(channel) = self.0.channel.borrow().as_ref() {
            channel.set_onopen(None);
            channel.set_onmessage(None);
            channel.set_onclose(None);
            channel.close();
        }
        self.0.pc.set_ondatachannel(None);
        self.0.pc.close();
    }
}
