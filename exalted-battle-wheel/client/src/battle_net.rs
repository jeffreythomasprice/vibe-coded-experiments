//! Server-authoritative multiplayer. `Battles` is the one facade every UI call site uses to
//! change the battle or manage room membership. The server owns every room's battle log and its
//! membership, checks every move for legality and permission, and this browser's own state is
//! never more than whatever the server's last message said it is. A websocket connection
//! (`crate::net::Socket`) carries every request and reply.

use crate::access::Access;
use crate::net::{Socket, SocketError};
use crate::persist::Persisted;
use leptos::prelude::*;
use leptos::wasm_bindgen::closure::Closure;
use leptos::wasm_bindgen::JsCast;
use leptos::web_sys;
use serde::{Deserialize, Serialize};
use shared::battle::{BattleError, BattleEvent, BattleLog};
use shared::protocol::{
    apply_command, sanitize_name, BattleCommand, BattleRequest, ClientEnvelope, ClientMessage, ConnectionId,
    LeaveReason, Member, ProtocolError, RequestId, RoomName, ServerEnvelope, ServerMessage, SessionRejection,
    MAX_ROOM_NAME_LEN,
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
/// the same name without the user doing anything — see `Battles::on_close`. Persisted to local
/// storage (see `Battles::new`'s own `rejoin` field), so this also survives a closed tab: `session`
/// is `None` until the server's first `Joined` reply fills it in, then carries this connection's
/// proof of membership (and host status, if it has it) for `Battles::resume` to present on the
/// very next page load.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Rejoin {
    room: String,
    name: String,
    session: Option<String>,
}

/// Trims and truncates to fit `RoomName`'s bound, mirroring `sanitize_name`'s "never reject
/// outright" policy -- but unlike a player name, an empty room name genuinely can't become a
/// `RoomName` (it has a `minLength`), so a blank or all-whitespace input falls back to a single
/// space rather than panicking; the server's own `room_key` then rejects that the same way it
/// rejects any other blank room name, via the ordinary `BadRoomName` error path this module
/// already handles. The room-name field also has a `maxlength` in the UI, so this only matters
/// for input that somehow bypasses it.
fn truncated_room_name(text: &str) -> RoomName {
    let trimmed = text.trim();
    let truncated = match trimmed.char_indices().nth(MAX_ROOM_NAME_LEN) {
        Some((end, _)) => &trimmed[..end],
        None => trimmed,
    };
    RoomName::try_from(truncated).unwrap_or_else(|_| RoomName::try_from(" ").expect("a single space always fits"))
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
    /// `Socket` wraps a `web_sys::WebSocket` (`Rc`-backed, so `!Send`) — hence `LocalStorage`.
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
    /// Persisted to local storage so a closed-and-reopened tab can find its way back to the room
    /// it left — see `Rejoin`'s own doc comment and `resume`. Never accepts a cross-tab `storage`
    /// event: two tabs in the same room would otherwise hand each other's session token back and
    /// forth, and each tab's session is specific to its own connection.
    rejoin: Persisted<Option<Rejoin>>,
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
            rejoin: Persisted::new_gated("room.session", || None, || false),
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

    /// Whether this connection is the room's host -- the one member who may hand out an invite
    /// link (`ui::room`'s `InviteLink`). Derived from the same membership broadcast `ui::room`'s
    /// own per-row badge reads, rather than tracked separately, so the two can never disagree.
    pub fn is_host(&self) -> Signal<bool> {
        let (members, self_id) = (self.members, self.self_id);
        Signal::derive(move || {
            let Some(id) = self_id.get() else { return false };
            members.with(|members| members.iter().any(|member| member.id == id && member.is_host))
        })
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

    /// The room a session left behind by a previous page load belongs to, if any -- what
    /// `crate::startup` compares an invite link's `join_room` against to decide whether to
    /// `resume()` (presenting the stored host token) or `join_room` the link's own room instead.
    pub fn stored_room(&self) -> Option<String> {
        self.rejoin.get_untracked().map(|rejoin| rejoin.room)
    }

    /// Claims `Mode::Connecting` before startup even knows whether this browser's access code
    /// still works, so the locally-persisted battle is never editable during that round trip -- an
    /// edit landed in the gap would only be thrown away the moment a room's own log replaces it.
    /// `room`, if given, is shown as the room the app is connecting to, the same way a reconnect's
    /// `ConnectingView` already does. Paired with `release_connecting`.
    pub fn hold_connecting(&self, room: Option<String>) {
        self.mode.set(Mode::Connecting);
        if room.is_some() {
            self.room.set(room);
        }
    }

    /// Backs out of a `hold_connecting` claim that turned out not to be needed (no access code,
    /// nothing to join or resume). Unlike `reset_to_solo`, this never touches the stored room
    /// session -- a browser that merely isn't signed in yet must still be able to find its way
    /// back to it once it is. A no-op once a socket exists: from that point on, whatever opened it
    /// (the user's own Host/Join, or `resume`/`join_room`) owns `mode`.
    pub fn release_connecting(&self) {
        if self.socket.get_untracked().is_some() {
            return;
        }
        self.mode.set(Mode::Solo);
        self.room.set(None);
    }

    pub fn create_room(&self, room: String, name: String, everyone_writes: bool) {
        let Some(socket) = self.ensure_socket() else { return };
        // No `session` yet -- the server's own `Joined` reply mints one, and `handle_message`
        // rewrites this with it.
        self.rejoin.set(Some(Rejoin { room: room.clone(), name: name.clone(), session: None }));
        self.mode.set(Mode::Connecting);
        let this = *self;
        let message = ClientMessage::Create {
            room: truncated_room_name(&room),
            name: sanitize_name(&name),
            everyone_writes,
            log: self.log.get_untracked(),
        };
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
        self.rejoin.set(Some(Rejoin { room: room.clone(), name: name.clone(), session: None }));
        self.mode.set(Mode::Connecting);
        let this = *self;
        let message = ClientMessage::Join { room: truncated_room_name(&room), name: sanitize_name(&name), session: None };
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
        self.send_action(ClientMessage::Rename { name: sanitize_name(&name) }, "rename");
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
            ServerMessage::Joined { room, you, can_write, everyone_writes, members, version: _, log, session } => {
                // Rebuilt from the server's own reply, not just the `session` field -- the name a
                // rejoin should present is this connection's own name as the server has it (found
                // by matching `you`), which a `Rename` since the last `Joined` may have changed.
                let name =
                    members.iter().find(|member| member.id == you).map(|member| member.name.to_string()).unwrap_or_default();
                self.rejoin.set(Some(Rejoin { room: room.to_string(), name, session: Some(session) }));

                self.log.set(log);
                self.self_id.set(Some(you));
                self.can_write.set(can_write);
                self.everyone_writes.set(everyone_writes);
                self.members.set(members);
                self.room.set(Some(room.to_string()));
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
                match reason {
                    LeaveReason::Requested => {}
                    LeaveReason::Kicked => crate::ui::toast::error("You were removed from the room.".to_string()),
                    LeaveReason::RoomClosed => {
                        crate::ui::toast::error("This room was closed by an administrator.".to_string())
                    }
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
        self.send_join(socket, rejoin);
    }

    /// Sends the `Join` a reconnect (or `resume`) needs on an already-open `socket`, and reacts
    /// to how it settles. Split out from `attempt_reconnect` so a merely stale `session` can retry
    /// once, immediately, on the same socket rather than going through the backoff -- see the
    /// `SessionRejection::Expired`/`Malformed` arm below.
    fn send_join(self, socket: Socket, rejoin: Rejoin) {
        let this = self;
        let sent_on = socket.clone();
        let message = ClientMessage::Join {
            room: truncated_room_name(&rejoin.room),
            name: sanitize_name(&rejoin.name),
            session: rejoin.session.clone(),
        };
        self.send_with(sent_on, message, move |result| {
            let Err(error) = result else { return };
            match error {
                // Unlike every other rejection here, this one will never stop recurring -- there's
                // no reason to believe the room will come back, so give up rather than retry into
                // it forever.
                RequestError::Rejected(ProtocolError::NoSuchRoom) => {
                    crate::ui::toast::error("This room no longer exists.".to_string());
                    this.disconnect();
                }
                // Also won't recur on its own: the access code this session was opened under has
                // since been revoked (`crate::startup` only reaches this path after confirming it
                // once, at page-load time -- this is a *later* revocation, mid-session).
                RequestError::Rejected(ProtocolError::Unauthorized) => {
                    crate::ui::toast::error("Your access code is no longer valid.".to_string());
                    this.disconnect();
                }
                RequestError::Rejected(ProtocolError::InvalidSession(rejection)) => {
                    crate::ui::toast::error(rejection.to_string());
                    match rejection {
                        // The room's name now belongs to a different room than this session was
                        // minted for -- an automatic rejoin must never walk into someone else's
                        // game on the strength of a token that predates it.
                        SessionRejection::WrongRoom => this.disconnect(),
                        // A merely stale token: the room itself is probably still fine, so retry
                        // once, immediately, without it -- landing as an ordinary member rather
                        // than host, but still in the room.
                        SessionRejection::Expired | SessionRejection::Malformed => {
                            let retry = Rejoin { session: None, ..rejoin };
                            this.rejoin.set(Some(retry.clone()));
                            this.send_join(socket, retry);
                        }
                    }
                }
                error => {
                    tracing::debug!(%error, "reconnect attempt failed");
                    this.schedule_reconnect(rejoin);
                }
            }
        });
    }

    /// Replays a room session left behind by a previous page load, presenting its host token if it
    /// has one -- so a returning host reclaims host rather than rejoining as an ordinary member.
    /// Called once at startup, from `crate::startup::run`, which has already confirmed this
    /// browser's access code still works -- unlike the `restore` this replaces, it no longer
    /// checks that itself, since deferring behind that check is exactly what `hold_connecting`
    /// already claimed `Mode::Connecting` for. Silent when there's nothing stored. A no-op if a
    /// socket already exists: a user who started their own Host/Join while the check was still in
    /// flight owns `mode` from that point on. The very first successful `create_room`/`join_room`
    /// overwrites whatever this resumes.
    pub fn resume(&self) {
        if self.socket.get_untracked().is_some() {
            return;
        }
        let Some(rejoin) = self.rejoin.get_untracked() else { return };
        self.room.set(Some(rejoin.room.clone()));
        self.mode.set(Mode::Connecting);
        self.attempt_reconnect(rejoin);
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
