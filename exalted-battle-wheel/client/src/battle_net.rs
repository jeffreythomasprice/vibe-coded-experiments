//! Server-authoritative multiplayer. `Battles` is the one facade every UI call site uses to
//! change the battle or manage room membership — replacing the old peer-to-peer `Session`, whose
//! whole two-phase-commit vote existed only because there was no server to be the single
//! authority. Now there is: the server owns every room's battle log and its membership, checks
//! every move for legality and permission, and this browser's own state is never more than
//! whatever the server's last message said it is. A websocket connection (`crate::net::Socket`)
//! carries every request and reply.

use crate::access::Access;
use crate::net::{Socket, SocketError};
use leptos::prelude::*;
use leptos::wasm_bindgen::closure::Closure;
use leptos::wasm_bindgen::JsCast;
use leptos::web_sys;
use shared::battle::{BattleError, BattleEvent, BattleLog};
use shared::protocol::{
    apply_command, BattleCommand, BattleRequest, ClientEnvelope, ClientMessage, ConnectionId, LeaveReason, Member,
    ProtocolError, RequestId, ServerEnvelope, ServerMessage,
};
use std::collections::HashMap;
use std::future::Future;
use std::sync::OnceLock;

/// The app's root reactive owner, captured once from `App()`'s own setup (see `app.rs`) and
/// re-entered by every deferred task this file spawns. Needed because `net::Socket`'s callbacks
/// (`on_message`, `on_close`) are invoked directly from raw `WebSocket` events — code that runs
/// entirely outside any Leptos-tracked call stack, so there is no "current owner" for
/// `leptos::task::spawn_local_scoped` to capture at the point it's actually called; it would
/// silently fall back to a disposable, context-less default `Owner`, under which `use_context`
/// (including inside `crate::ui::toast::error`) can only ever find nothing.
static ROOT_OWNER: OnceLock<Owner> = OnceLock::new();

pub fn set_root_owner(owner: Owner) {
    let _ = ROOT_OWNER.set(owner);
}

/// Every deferred handler in this file should go through here, never `leptos::task::spawn_local`
/// directly — see `ROOT_OWNER`'s doc comment. Also sidesteps a real wasm-bindgen hazard: several
/// branches below end by dropping the very `Socket` whose own callback is invoking them right now
/// (e.g. a `Left` message triggering `reset_to_solo`, which drops the `Socket` holding that
/// callback's `Closure`) — dropping a `Closure` while its `FnMut` is still on the call stack is a
/// hard panic, not a no-op. `spawn_local` queues this as a microtask, letting the callback that
/// invoked us finish and unwind first.
fn spawn_local(fut: impl Future<Output = ()> + 'static) {
    match ROOT_OWNER.get() {
        Some(owner) => owner.with(|| leptos::task::spawn_local_scoped(fut)),
        None => leptos::task::spawn_local_scoped(fut),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Solo,
    /// Either the very first `Create`/`Join` hasn't replied yet, or a connection that was
    /// `InRoom` just dropped and a reconnect attempt is under way — `room()`/`members()` still
    /// hold whatever they last did in the latter case, so the UI can say "reconnecting to X"
    /// rather than reverting to a blank form.
    Connecting,
    InRoom,
}

/// What a request's `Settle` callback is told when it fails. A typed enum rather than a bare
/// `String` so a caller like `join_room` can tell "the server rejected this" from "something else
/// tore the room down mid-request" and react differently -- see `worth_reporting`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RequestError {
    #[error("{0}")]
    Rejected(ProtocolError),
    #[error("{0}")]
    Local(BattleError),
    #[error("{0}")]
    Send(SocketError),
    #[error("still connecting to the room")]
    Connecting,
    #[error("you do not have write access in this room")]
    ReadOnly,
    #[error("not connected")]
    NotConnected,
    #[error("not signed in")]
    NotSignedIn,
    /// This browser walked away from the room (or the room turned out not to exist) while the
    /// request was still in flight -- `reset_to_solo`'s drain.
    #[error("left the room before this could be answered")]
    Abandoned,
    /// The connection dropped before a reply arrived -- `on_close`'s drain.
    #[error("disconnected before this could be answered")]
    Disconnected,
}

impl RequestError {
    /// `Abandoned` and `Disconnected` never happen on their own -- something else (a deliberate
    /// leave, a kick, a room that turned out not to exist) drained the request and has already
    /// said its piece -- so a toast here would only ever double up on a message the user has
    /// already seen.
    fn worth_reporting(&self) -> bool {
        !matches!(self, Self::Abandoned | Self::Disconnected)
    }
}

type Settle = Box<dyn FnOnce(Result<(), RequestError>)>;

/// What `create_room`/`join_room` remember so a dropped connection can rejoin the same room under
/// the same name without the user doing anything — see `Battles::on_close`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Rejoin {
    room: String,
    name: String,
}

const BASE_RECONNECT_DELAY_MS: i32 = 2_000;
const MAX_RECONNECT_DELAY_MS: i32 = 30_000;

fn reconnect_delay_ms(attempt: u32) -> i32 {
    BASE_RECONNECT_DELAY_MS.saturating_mul(1 << attempt.min(4)).min(MAX_RECONNECT_DELAY_MS)
}

/// The read-only view every UI component reads the battle log through. Context is keyed by type
/// alone, so replacing the writable `RwSignal<BattleLog>` that used to be provided with this
/// instead makes bypassing `Battles` a compile error, not just a convention.
pub type BattleView = ReadSignal<BattleLog>;

/// The one facade every UI call site uses to change the battle or manage room membership.
#[derive(Clone, Copy)]
pub struct Battles {
    log: RwSignal<BattleLog>,
    /// `Socket` wraps a `web_sys::WebSocket` (`Rc`-backed, so `!Send`) — hence `LocalStorage`,
    /// same reasoning as the old peer-to-peer `Session`'s own room state.
    socket: RwSignal<Option<Socket>, LocalStorage>,
    mode: RwSignal<Mode>,
    room: RwSignal<Option<String>>,
    self_id: RwSignal<Option<ConnectionId>>,
    can_write: RwSignal<bool>,
    everyone_writes: RwSignal<bool>,
    members: RwSignal<Vec<Member>>,
    /// Requests awaiting a reply, keyed by the id this browser minted for them. `Settle` isn't
    /// `Send`, hence `LocalStorage` — same reasoning as `socket`.
    pending: RwSignal<HashMap<RequestId, Settle>, LocalStorage>,
    next_request_id: RwSignal<u64>,
    rejoin: RwSignal<Option<Rejoin>>,
    reconnect_attempt: RwSignal<u32>,
    room_active: RwSignal<bool>,
}

impl Battles {
    /// `room_active` must be the same signal already passed to the battle log's
    /// `Persisted::new_gated` — see `app.rs`, which wires the two together.
    pub fn new(log: RwSignal<BattleLog>, room_active: RwSignal<bool>) -> Self {
        Self {
            log,
            socket: RwSignal::new_local(None),
            mode: RwSignal::new(Mode::Solo),
            room: RwSignal::new(None),
            self_id: RwSignal::new(None),
            can_write: RwSignal::new(false),
            everyone_writes: RwSignal::new(false),
            members: RwSignal::new(Vec::new()),
            pending: RwSignal::new_local(HashMap::new()),
            next_request_id: RwSignal::new(0),
            rejoin: RwSignal::new(None),
            reconnect_attempt: RwSignal::new(0),
            room_active,
        }
    }

    // --------------------------------------------------------------------------------- signals

    pub fn mode(&self) -> Signal<Mode> {
        let mode = self.mode;
        Signal::derive(move || mode.get())
    }

    /// The room's display name, once known — set the moment `Create`/`Join` succeeds, and kept
    /// (not cleared) through a `Mode::Connecting` reconnect so the UI can keep naming it.
    pub fn room(&self) -> Signal<Option<String>> {
        let room = self.room;
        Signal::derive(move || room.get())
    }

    pub fn self_id(&self) -> Signal<Option<ConnectionId>> {
        let self_id = self.self_id;
        Signal::derive(move || self_id.get())
    }

    pub fn members(&self) -> Signal<Vec<Member>> {
        let members = self.members;
        Signal::derive(move || members.get())
    }

    /// What a *future* joiner starts as — not retroactive to anyone already in the room. See
    /// `shared::protocol::ClientMessage::SetEveryoneWrites`'s doc comment.
    pub fn everyone_writes(&self) -> Signal<bool> {
        let everyone_writes = self.everyone_writes;
        Signal::derive(move || everyone_writes.get())
    }

    /// Whether this node may only watch. Driven by `can_write`, which every `Members` broadcast
    /// refreshes for this connection's own entry — so a promotion or demotion flips this, and
    /// everything gated on it, with no extra wiring. `Mode::Connecting` is always read-only: there
    /// is no permission to act on yet.
    pub fn read_only(&self) -> Signal<bool> {
        let mode = self.mode;
        let can_write = self.can_write;
        Signal::derive(move || match mode.get() {
            Mode::Solo => false,
            Mode::Connecting => true,
            Mode::InRoom => !can_write.get(),
        })
    }

    /// Whether a request is currently awaiting a reply — while in a room, editing the battle is a
    /// network round trip, not a local call, and the UI should hold off on starting a second
    /// change until the first has settled.
    pub fn busy(&self) -> Signal<bool> {
        let pending = self.pending;
        Signal::derive(move || pending.with(|pending| !pending.is_empty()))
    }

    // ------------------------------------------------------------------------------ game moves

    pub fn push(&self, event: BattleEvent) {
        self.propose(BattleRequest::Push(event), "push event");
    }

    pub fn push_minting(&self, event: BattleEvent) {
        self.propose(BattleRequest::PushMinting(event), "push event");
    }

    pub fn push_with(&self, event: BattleEvent, on_settled: impl FnOnce(Result<(), RequestError>) + 'static) {
        self.propose_with(BattleRequest::Push(event), on_settled);
    }

    pub fn push_minting_with(&self, event: BattleEvent, on_settled: impl FnOnce(Result<(), RequestError>) + 'static) {
        self.propose_with(BattleRequest::PushMinting(event), on_settled);
    }

    pub fn undo(&self) {
        self.propose_quiet(BattleRequest::Undo);
    }

    pub fn redo(&self) {
        self.propose_quiet(BattleRequest::Redo);
    }

    pub fn seek(&self, cursor: usize) {
        self.propose_quiet(BattleRequest::Seek(cursor));
    }

    pub fn reset(&self) {
        self.propose(BattleRequest::Reset, "reset battle");
    }

    fn propose(&self, request: BattleRequest, action: &'static str) {
        self.propose_with(request, move |result| {
            if let Err(error) = result {
                tracing::error!(%error, "could not {action}");
                if error.worth_reporting() {
                    crate::ui::toast::error(format!("Could not {action}: {error}"));
                }
            }
        });
    }

    /// For `undo`/`redo`/`seek`: their buttons are already disabled when there's nothing to do, so
    /// a rejection here only ever comes from a harmless race (another member's move landed first)
    /// rather than a user action gone wrong, and isn't worth a toast.
    fn propose_quiet(&self, request: BattleRequest) {
        self.propose_with(request, |result| {
            if let Err(error) = result {
                tracing::debug!(%error, "no-op");
            }
        });
    }

    fn propose_with(&self, request: BattleRequest, on_settled: impl FnOnce(Result<(), RequestError>) + 'static) {
        match self.mode.get_untracked() {
            Mode::Solo => {
                // No server to check with and nobody to disagree -- apply directly and
                // synchronously, exactly as editing your own solo battle always worked.
                let command = sequence_locally(&self.log, request);
                let mut result = Ok(());
                self.log.update(|log| result = apply_command(log, &command));
                on_settled(result.map_err(RequestError::Local));
            }
            Mode::Connecting => on_settled(Err(RequestError::Connecting)),
            Mode::InRoom => {
                if !self.can_write.get_untracked() {
                    on_settled(Err(RequestError::ReadOnly));
                    return;
                }
                let Some(socket) = self.socket.get_untracked() else {
                    on_settled(Err(RequestError::NotConnected));
                    return;
                };
                self.send_with(socket, ClientMessage::Request(request), on_settled);
            }
        }
    }

    // --------------------------------------------------------------------------- room lifecycle

    pub fn create_room(&self, room: String, name: String, everyone_writes: bool) {
        let Some(socket) = self.ensure_socket() else { return };
        self.rejoin.set(Some(Rejoin { room: room.clone(), name: name.clone() }));
        self.mode.set(Mode::Connecting);
        let this = *self;
        let message = ClientMessage::Create { room, name, everyone_writes, log: self.log.get_untracked() };
        self.send_with(socket, message, move |result| {
            if let Err(error) = result {
                tracing::error!(%error, "could not create room");
                if error.worth_reporting() {
                    crate::ui::toast::error(format!("Could not create room: {error}"));
                }
                this.reset_to_solo();
            }
        });
    }

    pub fn join_room(&self, room: String, name: String) {
        let Some(socket) = self.ensure_socket() else { return };
        self.rejoin.set(Some(Rejoin { room: room.clone(), name: name.clone() }));
        self.mode.set(Mode::Connecting);
        let this = *self;
        let message = ClientMessage::Join { room, name };
        self.send_with(socket, message, move |result| {
            if let Err(error) = result {
                tracing::error!(%error, "could not join room");
                if matches!(error, RequestError::Rejected(ProtocolError::NoSuchRoom)) {
                    crate::ui::toast::error("No such room exists.".to_string());
                } else if error.worth_reporting() {
                    crate::ui::toast::error(format!("Could not join room: {error}"));
                }
                this.reset_to_solo();
            }
        });
    }

    pub fn leave(&self) {
        if self.mode.get_untracked() == Mode::Solo {
            return;
        }
        if let Some(socket) = self.socket.get_untracked()
            && let Some((_, envelope)) = self.envelope(ClientMessage::Leave)
        {
            let _ = socket.send(&envelope);
        }
        self.disconnect();
    }

    pub fn rename(&self, name: String) {
        self.send_action(ClientMessage::Rename { name }, "rename");
    }

    pub fn set_writable(&self, member: ConnectionId, can_write: bool) {
        self.send_action(ClientMessage::SetWritable { member, can_write }, "change write access");
    }

    pub fn set_everyone_writes(&self, everyone_writes: bool) {
        self.send_action(ClientMessage::SetEveryoneWrites { everyone_writes }, "change the room's default");
    }

    pub fn kick(&self, member: ConnectionId) {
        self.send_action(ClientMessage::Kick { member }, "remove that member");
    }

    /// Asks the server to resend this room's membership and battle wholesale, in case a broadcast
    /// was ever missed. Available to a readonly member too — it changes nothing, it only asks.
    pub fn resync(&self) {
        self.send_action(ClientMessage::Resync, "refresh the room");
    }

    fn send_action(&self, message: ClientMessage, action: &'static str) {
        let Some(socket) = self.socket.get_untracked() else { return };
        self.send_with(socket, message, move |result| {
            if let Err(error) = result {
                tracing::error!(%error, "could not {action}");
                if error.worth_reporting() {
                    crate::ui::toast::error(format!("Could not {action}: {error}"));
                }
            }
        });
    }

    // -------------------------------------------------------------------------------- transport

    /// Returns the current socket, opening a new one first if none exists yet — hosting or
    /// joining a room starts a connection on demand rather than the app carrying one from launch.
    fn ensure_socket(&self) -> Option<Socket> {
        if let Some(socket) = self.socket.get_untracked() {
            return Some(socket);
        }
        let this = *self;
        match Socket::connect(move |envelope| this.on_message(envelope), move || this.on_close()) {
            Ok(socket) => {
                self.socket.set(Some(socket.clone()));
                Some(socket)
            }
            Err(error) => {
                tracing::error!(%error, "could not open a connection");
                crate::ui::toast::error(format!("Could not connect: {error}"));
                None
            }
        }
    }

    fn next_request_id(&self) -> RequestId {
        let mut id = 0;
        self.next_request_id.update(|next| {
            id = *next;
            *next += 1;
        });
        RequestId(id)
    }

    /// Builds an envelope for `message` using the currently signed-in access token. `None` if
    /// there isn't one — every path that can reach this already requires being signed in (the
    /// room UI itself is unreachable without a token; see `ui::room`), so this is a defensive
    /// no-op rather than a real path a user can hit.
    fn envelope(&self, message: ClientMessage) -> Option<(RequestId, ClientEnvelope)> {
        let token = expect_context::<Access>().token().get_untracked()?;
        let id = self.next_request_id();
        Some((id, ClientEnvelope { id, token, message }))
    }

    fn send_with(&self, socket: Socket, message: ClientMessage, on_settled: impl FnOnce(Result<(), RequestError>) + 'static) {
        let Some((id, envelope)) = self.envelope(message) else {
            on_settled(Err(RequestError::NotSignedIn));
            return;
        };
        self.pending.update(|pending| {
            pending.insert(id, Box::new(on_settled));
        });
        if let Err(error) = socket.send(&envelope) {
            self.settle(id, Err(RequestError::Send(error)));
        }
    }

    fn settle(&self, request_id: RequestId, result: Result<(), RequestError>) {
        let callback = self.pending.try_update(|pending| pending.remove(&request_id)).flatten();
        if let Some(callback) = callback {
            callback(result);
        }
    }

    /// Clears every trace of a room — leaving deliberately, being kicked, giving up on a
    /// reconnect, or a room turning out not to exist anymore all funnel through here, so
    /// `on_close` never mistakes any of them for a connection worth reconnecting. Every one of
    /// those call sites closes the socket first (detaching its handlers — see `Socket::close`'s
    /// doc comment), so `on_close`'s own pending-request drain never runs for them; this drains
    /// the same way itself; a request still in flight when this fires would otherwise sit in
    /// `pending` forever, wedging `busy()` at `true` for the rest of the session.
    fn reset_to_solo(&self) {
        let stranded: Vec<Settle> =
            self.pending.try_update(|pending| pending.drain().map(|(_, settle)| settle).collect()).unwrap_or_default();
        for settle in stranded {
            settle(Err(RequestError::Abandoned));
        }

        self.rejoin.set(None);
        self.reconnect_attempt.set(0);
        self.mode.set(Mode::Solo);
        self.room.set(None);
        self.self_id.set(None);
        self.can_write.set(false);
        self.everyone_writes.set(false);
        self.members.set(Vec::new());
        self.room_active.set(false);
    }

    /// Closes the socket -- detaching its handlers first (see `Socket::close`'s doc comment) so
    /// this deliberate disconnect is never mistaken for one to reconnect from -- and clears every
    /// trace of the room. The common tail of `leave()`, being kicked, and a room turning out not
    /// to exist while this connection was in (or entering) it.
    fn disconnect(&self) {
        if let Some(socket) = self.socket.get_untracked() {
            socket.close();
        }
        self.socket.set(None);
        self.reset_to_solo();
    }

    // ------------------------------------------------------------------------------ callbacks

    fn on_message(self, envelope: ServerEnvelope) {
        spawn_local(async move { self.handle_message(envelope) });
    }

    fn handle_message(self, envelope: ServerEnvelope) {
        let ServerEnvelope { reply_to, message } = envelope;
        let result = match message {
            ServerMessage::Joined { room, you, can_write, everyone_writes, members, version: _, log } => {
                self.log.set(log);
                self.self_id.set(Some(you));
                self.can_write.set(can_write);
                self.everyone_writes.set(everyone_writes);
                self.members.set(members);
                self.room.set(Some(room));
                self.mode.set(Mode::InRoom);
                self.room_active.set(true);
                self.reconnect_attempt.set(0);
                Ok(())
            }
            ServerMessage::Members { members, everyone_writes } => {
                if let Some(self_id) = self.self_id.get_untracked()
                    && let Some(member) = members.iter().find(|member| member.id == self_id)
                {
                    self.can_write.set(member.can_write);
                }
                self.members.set(members);
                self.everyone_writes.set(everyone_writes);
                Ok(())
            }
            ServerMessage::State { room: _, version: _, log } => {
                self.log.set(log);
                Ok(())
            }
            ServerMessage::Left { reason } => {
                if reason == LeaveReason::Kicked {
                    crate::ui::toast::error("You were removed from the room.".to_string());
                }
                self.disconnect();
                Ok(())
            }
            ServerMessage::Error { error } => {
                // A room that's gone by the time this reached the server means whatever this
                // browser thought it was in is stale -- drop it locally too, the same way the
                // server's own connection handler does (see `ws::mod`'s doc comment). A
                // `NoSuchRoom` hit while still `Connecting` isn't staleness -- it's an initial
                // `join_room` or a reconnect attempt finding the room gone, and both of those
                // handle it themselves (see `join_room` and `attempt_reconnect`) so each can say
                // the right thing in exactly one toast.
                if error == ProtocolError::NoSuchRoom && self.mode.get_untracked() == Mode::InRoom {
                    crate::ui::toast::error("This room no longer exists.".to_string());
                    self.disconnect();
                }
                Err(RequestError::Rejected(error))
            }
        };
        if let Some(request_id) = reply_to {
            self.settle(request_id, result);
        }
    }

    fn on_close(self) {
        spawn_local(async move {
            self.socket.set(None);
            // Anything still waiting on a reply from this connection will never hear back.
            let stranded: Vec<Settle> =
                self.pending.try_update(|pending| pending.drain().map(|(_, settle)| settle).collect()).unwrap_or_default();
            for settle in stranded {
                settle(Err(RequestError::Disconnected));
            }

            match self.rejoin.get_untracked() {
                Some(rejoin) => {
                    self.mode.set(Mode::Connecting);
                    self.schedule_reconnect(rejoin);
                }
                // A deliberate `leave()`, or a `create_room`/`join_room` that never succeeded,
                // already cleared `rejoin` (and the rest of the room state with it) by the time
                // we get here -- nothing left to do.
                None => {}
            }
        });
    }

    fn schedule_reconnect(self, rejoin: Rejoin) {
        let attempt = self.reconnect_attempt.get_untracked();
        self.reconnect_attempt.set(attempt + 1);
        let delay = reconnect_delay_ms(attempt);

        let closure = Closure::once(move || self.attempt_reconnect(rejoin));
        if let Some(window) = web_sys::window() {
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(closure.as_ref().unchecked_ref(), delay);
        }
        // One-shot: nothing else holds this closure alive, and the timeout above is the only
        // thing that will ever call it.
        closure.forget();
    }

    fn attempt_reconnect(self, rejoin: Rejoin) {
        // Still relevant? A `leave()` (or a fresh `create_room`/`join_room`) in the meantime
        // clears or replaces `rejoin`, and a stale attempt should quietly give up rather than
        // reconnect into a room the user already walked away from.
        if self.rejoin.get_untracked().as_ref() != Some(&rejoin) {
            return;
        }
        let Some(socket) = self.ensure_socket() else {
            self.schedule_reconnect(rejoin);
            return;
        };
        let this = self;
        let message = ClientMessage::Join { room: rejoin.room.clone(), name: rejoin.name.clone() };
        self.send_with(socket, message, move |result| {
            if let Err(error) = result {
                // Unlike every other rejection here, this one will never stop recurring --
                // there's no reason to believe the room will come back, so give up rather than
                // retry into it forever.
                if matches!(error, RequestError::Rejected(ProtocolError::NoSuchRoom)) {
                    crate::ui::toast::error("This room no longer exists.".to_string());
                    this.disconnect();
                    return;
                }
                tracing::debug!(%error, "reconnect attempt failed");
                this.schedule_reconnect(rejoin);
            }
        });
    }
}

/// Solo-mode equivalent of the server's own `sequence` step (`server/src/ws/handler.rs`): turns a
/// `BattleRequest` into the concrete `BattleCommand` every node applies identically. Duplicated
/// rather than shared because the two sides differ in exactly one way — who mints a `PushMinting`
/// event's placeholder ids — and sharing would mean threading a `BattleLog` reference through a
/// function neither side calls with the same intent otherwise.
fn sequence_locally(log: &RwSignal<BattleLog>, request: BattleRequest) -> BattleCommand {
    match request {
        BattleRequest::Push(event) => BattleCommand::Push(event),
        BattleRequest::PushMinting(event) => BattleCommand::Push(log.read_untracked().restamp(event)),
        BattleRequest::Undo => BattleCommand::Undo,
        BattleRequest::Redo => BattleCommand::Redo,
        BattleRequest::Seek(cursor) => BattleCommand::Seek(cursor),
        BattleRequest::Reset => BattleCommand::Reset,
    }
}
