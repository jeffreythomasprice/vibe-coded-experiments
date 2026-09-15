use crate::net::message::{AbortReason, Message, VoteKind};
use crate::net::rtc::sleep_ms;
use crate::net::{Link, Replicated, RoomError, TxnId};
use leptos::prelude::*;
use shared::protocol::{PeerId, PeerInfo, StateHash};
use std::collections::VecDeque;
use std::future::Future;
use std::sync::OnceLock;

/// The app's root reactive owner, captured once from `App()`'s own setup (see `app.rs`) and
/// re-entered by every deferred task this file spawns. Needed because several of these handlers
/// (`on_host_message`, `on_joined_message`, both disconnect handlers) are invoked directly from
/// raw `RTCDataChannel`/`RTCPeerConnection` callbacks — code that runs entirely outside any
/// Leptos-tracked call stack, so there is no "current owner" for `leptos::task::spawn_local_scoped`
/// to capture at the point it's actually called; it would silently fall back to a disposable,
/// context-less default `Owner`, under which `use_context` (including inside
/// `crate::ui::toast::error`) can only ever find nothing. Confirmed live: a `Kick` toast never
/// rendered under exactly that fallback, with no panic or error — only re-entering the real root
/// owner first fixes it.
static ROOT_OWNER: OnceLock<Owner> = OnceLock::new();

pub fn set_root_owner(owner: Owner) {
    let _ = ROOT_OWNER.set(owner);
}

/// Every deferred handler in this file should go through here, never `leptos::task::spawn_local`
/// directly — see `ROOT_OWNER`'s doc comment for why a plain call can silently lose the ability to
/// find any context.
fn spawn_local(fut: impl Future<Output = ()> + 'static) {
    match ROOT_OWNER.get() {
        Some(owner) => owner.with(|| leptos::task::spawn_local_scoped(fut)),
        None => leptos::task::spawn_local_scoped(fut),
    }
}

/// Shorthand for the concrete `Message` type this app speaks — spelled out once here so call
/// sites never need to name all three of `A`'s associated types themselves.
type Msg<A> = Message<<A as Replicated>::Request, <A as Replicated>::Command, <A as Replicated>::Snapshot>;

/// How long the host waits for every vote before giving up on whoever hasn't answered. Generous
/// for a same-room, low-latency link; a real network hiccup is exactly what this exists to catch.
const VOTE_TIMEOUT_MS: i32 = 5000;

/// How long the host waits, once it has accepted a joiner's answer, for ICE to actually finish
/// connecting before giving up. Signaling is already done by this point — both descriptions are
/// set and there's nothing left but for the browsers to reach each other, which with no TURN relay
/// configured either succeeds within a few seconds or never will.
const CONNECT_TIMEOUT_MS: i32 = 15000;

/// How long the joiner waits, after producing its own answer, for a `Welcome` to arrive. Unlike
/// `CONNECT_TIMEOUT_MS`, this window still has a human step inside it: the joiner's answer has to
/// be copied back to the host and pasted into `accept_answer` before the host's own ICE agent even
/// has anything to connect to, and only then does the `Welcome` round trip happen. A short deadline
/// here doesn't catch a slow network — it catches a slow human, misreported as one — so this is
/// deliberately much more generous than `CONNECT_TIMEOUT_MS`.
const JOIN_REPLY_TIMEOUT_MS: i32 = 60000;

fn report_connect_timeout() {
    crate::ui::toast::error(
        "Could not connect \u{2014} this may be a restrictive network (no relay server is configured, only a direct connection)".to_string(),
    );
}

type Settle = Box<dyn FnOnce(Result<(), String>)>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Solo,
    Hosting,
    Joined,
}

/// What this node may do in the room it's currently in. Meaningless in `Mode::Solo` — defaults to
/// `Host` there, since editing your own solo battle is always allowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Host,
    Admin,
    Spectator,
}

fn report(action: &'static str, error: RoomError) {
    tracing::error!(%error, "could not {action}");
    crate::ui::toast::error(format!("Could not {action}: {error}"));
}

fn vote_before(vote: &VoteKind) -> StateHash {
    match vote {
        VoteKind::Yes { before, .. } | VoteKind::No { before, .. } => *before,
    }
}

struct ConnectedPeer {
    info: PeerInfo,
    link: Link,
}

/// A proposal waiting its turn — the host processes exactly one at a time (`in_flight`), so
/// anything else that arrives while one is already running just waits here.
struct QueuedProposal<A: Replicated> {
    txn: TxnId,
    origin: PeerId,
    request: A::Request,
    on_settled: Option<Settle>,
}

/// The one proposal currently being voted on. `votes` starts with the host's own (computed
/// synchronously, no wire round trip) and fills in as `Vote` messages arrive; `expected` is the
/// peer set a decision is waiting on. `on_settled` is `Some` only if this session itself is
/// `origin` — everyone else just applies the eventual `Commit`/`Abort`, no callback to run.
struct InFlight<A: Replicated> {
    txn: TxnId,
    origin: PeerId,
    command: A::Command,
    before: StateHash,
    votes: Vec<(PeerId, VoteKind)>,
    expected: Vec<PeerId>,
    on_settled: Option<Settle>,
}

/// The room's actual connections and in-flight agreement state. Lives behind
/// `RwSignal<_, LocalStorage>` since `Link` holds non-`Send` WebRTC handles.
enum RoomState<A: Replicated> {
    Solo,
    Hosting {
        self_id: PeerId,
        self_name: String,
        everyone_admin: bool,
        peers: Vec<ConnectedPeer>,
        /// The one live, not-yet-joined invite. Regenerated every time a peer joins (or a pending
        /// connection dies before joining), so there is always exactly one usable invite.
        pending: Option<(PeerId, Link)>,
        /// Bumped by every `create_invite` call and captured by its own async task, so that if two
        /// invite-generation attempts are ever in flight at once (`on_hello`'s auto-regeneration is
        /// the only case that can race like this), whichever one finishes second can tell it's no
        /// longer current and close its now-orphaned connection instead of clobbering `pending`
        /// with a `Link` nobody has the matching offer for.
        invite_generation: u64,
        queue: VecDeque<QueuedProposal<A>>,
        in_flight: Option<InFlight<A>>,
    },
    Joined {
        /// Placeholder until `Welcome` names them; a random real id will never collide with it.
        self_id: PeerId,
        link: Link,
        /// This node's one outstanding proposal, if any — a peer only ever waits on one at a time
        /// in practice (the UI gates on `busy`), but see `joined_propose` for the defensive case.
        awaiting: Option<(TxnId, Settle)>,
    },
}

fn random_peer_id() -> PeerId {
    let mut bytes = [0u8; 8];
    if let Some(crypto) = web_sys::window().and_then(|window| window.crypto().ok()) {
        let _ = crypto.get_random_values_with_u8_array(&mut bytes);
    }
    PeerId(u64::from_le_bytes(bytes))
}

fn roster_of(self_id: PeerId, self_name: &str, peers: &[ConnectedPeer]) -> Vec<PeerInfo> {
    std::iter::once(PeerInfo { id: self_id, name: self_name.to_string(), admin: true })
        .chain(peers.iter().map(|peer| peer.info.clone()))
        .collect()
}

/// Every roster entry's name passes through here before it's stored — a name is replicated to
/// every peer on every change, so an unbounded one makes one peer's typing everyone else's
/// bandwidth. Not attempting to catch anything more than length: this is a display label, not a
/// security boundary.
const MAX_NAME_LEN: usize = 40;

fn sanitize_name(name: String) -> String {
    let trimmed = name.trim();
    match trimmed.char_indices().nth(MAX_NAME_LEN) {
        Some((end, _)) => trimmed[..end].to_string(),
        None => trimmed.to_string(),
    }
}

/// Drives a `Replicated` app through proposals, and — once hosting or joined — through a room's
/// membership and the two-phase-commit vote that keeps every node's copy in agreement. In
/// `Mode::Solo`, `propose` still settles immediately via `sequence` + `commit`: there is nobody
/// else to agree with.
#[derive(Clone, Copy)]
pub struct Session<A: Replicated> {
    app: A,
    state: RwSignal<RoomState<A>, LocalStorage>,
    mode: RwSignal<Mode>,
    role: RwSignal<Role>,
    self_id: RwSignal<Option<PeerId>>,
    /// Who the room answers to. `Some` for everyone in a room, including the host itself (its own
    /// id); `None` in `Mode::Solo`. The roster synthesizes the host as entry 0, but nothing on the
    /// wire marks it as such, so the UI needs this to tell the host's row apart from any other
    /// admin's.
    host_id: RwSignal<Option<PeerId>>,
    peers: RwSignal<Vec<PeerInfo>>,
    invite: RwSignal<Option<String>>,
    answer_code: RwSignal<Option<String>>,
    room_active: RwSignal<bool>,
    next_txn: RwSignal<u64>,
}

impl<A: Replicated> Session<A> {
    /// `room_active` is created by the caller (before the app's own persisted state, in practice)
    /// rather than here, so that state's own cross-tab gating can read it without a
    /// construction-order cycle: the gate needs to exist before `Session::new` can run, since
    /// `Session` is built from the very state that gate protects.
    pub fn new(app: A, room_active: RwSignal<bool>) -> Self {
        Self {
            app,
            state: RwSignal::new_local(RoomState::Solo),
            mode: RwSignal::new(Mode::Solo),
            role: RwSignal::new(Role::Host),
            self_id: RwSignal::new(None),
            host_id: RwSignal::new(None),
            peers: RwSignal::new(Vec::new()),
            invite: RwSignal::new(None),
            answer_code: RwSignal::new(None),
            room_active,
            next_txn: RwSignal::new(0),
        }
    }

    fn next_txn_id(&self) -> TxnId {
        let mut id = 0;
        self.next_txn.update(|next| {
            id = *next;
            *next += 1;
        });
        TxnId(id)
    }

    pub fn mode(&self) -> Signal<Mode> {
        let mode = self.mode;
        Signal::derive(move || mode.get())
    }

    pub fn role(&self) -> Signal<Role> {
        let role = self.role;
        Signal::derive(move || role.get())
    }

    pub fn self_id(&self) -> Signal<Option<PeerId>> {
        let self_id = self.self_id;
        Signal::derive(move || self_id.get())
    }

    pub fn host_id(&self) -> Signal<Option<PeerId>> {
        let host_id = self.host_id;
        Signal::derive(move || host_id.get())
    }

    pub fn peers(&self) -> Signal<Vec<PeerInfo>> {
        let peers = self.peers;
        Signal::derive(move || peers.get())
    }

    pub fn invite(&self) -> Signal<Option<String>> {
        let invite = self.invite;
        Signal::derive(move || invite.get())
    }

    pub fn answer_code(&self) -> Signal<Option<String>> {
        let answer_code = self.answer_code;
        Signal::derive(move || answer_code.get())
    }

    /// Whether the host is currently sitting on a live invite nobody has claimed yet — true for
    /// essentially the whole time this end is `Hosting`, since a fresh invite is always kept
    /// live for one more joiner (see `pending` on `RoomState::Hosting`). Lets the UI show that
    /// this end is waiting on someone else, rather than just a blank "paste a code here" form.
    pub fn awaiting_peer(&self) -> Signal<bool> {
        let state = self.state;
        Signal::derive(move || state.with(|state| matches!(state, RoomState::Hosting { pending: Some(_), .. })))
    }

    /// Whether this node may only watch. A spectator's copy stays in step with the room and it
    /// votes on every `Prepare` like everyone else — it just may not propose a change: the host
    /// answers a non-admin's `Propose` with an `Abort` (see `host_on_propose`). Driven by `role`,
    /// which is re-derived from the roster on every `Roster`/`Welcome` (`sync_role_from_roster`),
    /// so a promotion or demotion flips this — and everything gated on it — with no extra wiring.
    pub fn read_only(&self) -> Signal<bool> {
        let role = self.role;
        Signal::derive(move || role.get() == Role::Spectator)
    }

    /// Whether this node has a proposal currently awaiting agreement.
    pub fn busy(&self) -> Signal<bool> {
        let state = self.state;
        Signal::derive(move || {
            state.with(|state| match state {
                RoomState::Hosting { in_flight, .. } => in_flight.is_some(),
                RoomState::Joined { awaiting, .. } => awaiting.is_some(),
                RoomState::Solo => false,
            })
        })
    }

    // ---------------------------------------------------------------- propose (all three modes)

    pub fn propose(&self, request: A::Request, on_settled: impl FnOnce(Result<(), String>) + 'static) {
        match self.mode.get_untracked() {
            Mode::Solo => {
                let result = self.app.sequence(request).and_then(|command| self.app.commit(&command));
                on_settled(result.map_err(|error| error.to_string()));
            }
            Mode::Hosting => {
                let Some(self_id) = self.self_id.get_untracked() else { return };
                let txn = self.next_txn_id();
                self.host_enqueue(txn, self_id, request, Some(Box::new(on_settled)));
            }
            Mode::Joined => self.joined_propose(request, Box::new(on_settled)),
        }
    }

    // ----------------------------------------------------- membership (hosting and joined both)

    /// Renames this node in whatever room it's in — a no-op in `Mode::Solo`, so the room UI can
    /// call this unconditionally regardless of which branch is showing. The host is the only node
    /// that edits the roster, so a joiner asks rather than tells: nobody, including the caller,
    /// sees the new name until the host's own `Roster` broadcast carries it back around.
    pub fn rename(&self, name: String) {
        let name = sanitize_name(name);
        if name.is_empty() {
            return;
        }
        match self.mode.get_untracked() {
            Mode::Solo => {}
            Mode::Hosting => {
                let Some(self_id) = self.self_id.get_untracked() else { return };
                self.host_set_name(self_id, name);
            }
            Mode::Joined => self.send_to_host(&Msg::<A>::Rename { name }),
        }
    }

    /// Promotes or demotes another peer. The host and a peer both funnel into `host_set_admin`,
    /// which is where every rule about who may do this to whom is checked — a peer's own UI
    /// hiding the control is a convenience, not the enforcement.
    pub fn set_admin(&self, peer_id: PeerId, admin: bool) {
        match self.mode.get_untracked() {
            Mode::Solo => {}
            Mode::Hosting => {
                let Some(self_id) = self.self_id.get_untracked() else { return };
                self.host_set_admin(self_id, peer_id, admin);
            }
            Mode::Joined => self.send_to_host(&Msg::<A>::SetAdmin { peer: peer_id, admin }),
        }
    }

    fn send_to_host(self, message: &Msg<A>) {
        let Some(link) = self.joined_link() else { return };
        if let Ok(json) = serde_json::to_string(message) {
            let _ = link.send(&json);
        }
    }

    // --------------------------------------------------------------------------------- hosting

    pub fn host(&self, name: String, everyone_admin: bool) {
        let this = *self;
        let self_id = random_peer_id();
        this.peers.set(roster_of(self_id, &name, &[]));
        this.state.set(RoomState::Hosting {
            self_id,
            self_name: name,
            everyone_admin,
            peers: Vec::new(),
            pending: None,
            invite_generation: 0,
            queue: VecDeque::new(),
            in_flight: None,
        });
        this.mode.set(Mode::Hosting);
        this.role.set(Role::Host);
        this.self_id.set(Some(self_id));
        this.host_id.set(Some(self_id));
        this.room_active.set(true);
        this.invite.set(None);
        this.create_invite();
    }

    fn create_invite(self) {
        // Cleared immediately, not left showing the just-consumed code: that code now belongs to
        // an already-`stable` peer connection, and handing it to a second joiner produces a
        // confusing, unrelated failure (`setRemoteDescription` rejecting an answer against a
        // connection that already has one) instead of an honest "not ready yet".
        self.invite.set(None);
        let peer_id = random_peer_id();
        let mut generation = 0;
        self.state.update(|state| {
            if let RoomState::Hosting { invite_generation, .. } = state {
                *invite_generation += 1;
                generation = *invite_generation;
            }
        });
        spawn_local(async move {
            let on_message = move |link: Link, text: String| self.on_host_message(peer_id, link, text);
            let on_close = move || self.on_host_peer_closed(peer_id);
            match Link::host(|_link| {}, on_message, on_close).await {
                Ok((link, code)) => {
                    let mut still_current = false;
                    self.state.update(|state| {
                        if let RoomState::Hosting { pending, invite_generation, .. } = state
                            && *invite_generation == generation
                        {
                            *pending = Some((peer_id, link.clone()));
                            still_current = true;
                        }
                    });
                    if still_current {
                        self.invite.set(Some(code));
                    } else {
                        // Either `leave()`/a second `host()` ran while this was negotiating, or
                        // another `create_invite` call already won the race for this slot — this
                        // connection has no home to go to. Close it explicitly rather than just
                        // dropping it: an unclosed `RtcPeerConnection` keeps trying to negotiate in
                        // the background instead of releasing its ICE/DTLS resources immediately.
                        link.close();
                    }
                }
                Err(error) => {
                    report("create an invite", error);
                    // A room nobody has joined yet is dead without a usable invite — reset it
                    // rather than leaving `mode` stuck on `Hosting` with `invite` forever `None`.
                    // A room with connected peers stays up; they just don't get a fresh invite.
                    let empty = self.state.with_untracked(|state| match state {
                        RoomState::Hosting { peers, .. } => peers.is_empty(),
                        _ => false,
                    });
                    if empty {
                        self.reset_to_solo();
                    }
                }
            }
        });
    }

    /// Completes hosting once the joiner has pasted back their answer.
    pub fn accept_answer(&self, code: String) {
        let this = *self;
        let pending_peer = this.state.with_untracked(|state| match state {
            RoomState::Hosting { pending: Some((peer_id, link)), .. } => Some((*peer_id, link.clone())),
            _ => None,
        });
        let Some((peer_id, link)) = pending_peer else { return };
        spawn_local(async move {
            match link.accept_answer(&code).await {
                // Setting the remote description only starts ICE; the actual connection (and
                // `on_hello`, which clears `pending`) can still fail to ever arrive.
                Ok(()) => this.host_schedule_connect_timeout(peer_id),
                Err(error) => report("connect to that answer", error),
            }
        });
    }

    /// Gives up on a pending peer that never completed the handshake after accepting their
    /// answer — either ICE never connected, or the data channel never opened. A no-op if
    /// `on_hello` (or a fresh `create_invite`) already moved this peer slot on.
    fn host_schedule_connect_timeout(self, peer_id: PeerId) {
        spawn_local(async move {
            sleep_ms(CONNECT_TIMEOUT_MS).await;
            let stale = self.state.with_untracked(|state| match state {
                RoomState::Hosting { pending: Some((id, link)), .. } if *id == peer_id => Some(link.clone()),
                _ => None,
            });
            let Some(link) = stale else { return };
            link.close();
            self.state.update(|state| {
                if let RoomState::Hosting { pending, .. } = state
                    && matches!(pending, Some((id, _)) if *id == peer_id)
                {
                    *pending = None;
                }
            });
            report_connect_timeout();
            self.create_invite();
        });
    }

    /// Deferred via `spawn_local` rather than handled inline: several branches here can end with
    /// `self` dropping the very `Link` whose own `on_message` closure is invoking this callback
    /// right now. Dropping a `Closure` while its underlying `FnMut` is still on the call stack is
    /// a hard wasm-bindgen panic ("invoked recursively or after being dropped"), not a no-op — so
    /// nothing that might drop a link's last reference may run synchronously inside its own
    /// callback. `spawn_local` queues this as a microtask, letting the callback that invoked us
    /// finish and unwind (dropping its own temporary references) before this ever runs.
    fn on_host_message(self, peer_id: PeerId, link: Link, text: String) {
        spawn_local(async move {
            let Ok(message) = serde_json::from_str::<Msg<A>>(&text) else {
                tracing::debug!(peer = peer_id.0, "ignoring an undecodable message");
                return;
            };
            match message {
                Message::Hello { name } => self.on_hello(peer_id, link, sanitize_name(name)),
                Message::Bye => self.remove_peer(peer_id),
                Message::Rename { name } => self.host_set_name(peer_id, sanitize_name(name)),
                Message::SetAdmin { peer, admin } => self.host_set_admin(peer_id, peer, admin),
                Message::Propose { txn, request } => self.host_on_propose(peer_id, txn, request),
                Message::Vote { txn, vote } => self.host_on_vote(peer_id, txn, vote),
                // A peer never legitimately sends these; there is nothing to act on either way.
                Message::Welcome { .. } | Message::Roster { .. } | Message::Kick | Message::Prepare { .. } | Message::Commit { .. } | Message::Abort { .. } => {}
            }
        });
    }

    fn on_hello(self, peer_id: PeerId, link: Link, name: String) {
        let snapshot = self.app.snapshot();
        let mut welcome_context = None;
        self.state.update(|state| {
            let RoomState::Hosting { self_id, self_name, everyone_admin, peers, pending, .. } = state else { return };
            if !matches!(pending, Some((id, _)) if *id == peer_id) {
                return;
            }
            *pending = None;
            peers.push(ConnectedPeer { info: PeerInfo { id: peer_id, name, admin: *everyone_admin }, link: link.clone() });
            welcome_context = Some((*self_id, roster_of(*self_id, self_name, peers)));
        });
        let Some((host_id, roster)) = welcome_context else { return };
        let welcome = Msg::<A>::Welcome { you: peer_id, host: host_id, roster: roster.clone(), snapshot };
        if let Ok(json) = serde_json::to_string(&welcome) {
            let _ = link.send(&json);
        }
        self.peers.set(roster);
        self.broadcast_roster(Some(peer_id));
        self.create_invite();
    }

    /// The host owns every name in the room; a joiner's `Rename` and the host's own `rename` both
    /// land here. Skips the rebroadcast when nothing actually changed, since `on:change` fires on
    /// every blur regardless of whether the field was actually edited.
    fn host_set_name(self, peer_id: PeerId, name: String) {
        let mut roster = None;
        self.state.update(|state| {
            let RoomState::Hosting { self_id, self_name, peers, .. } = state else { return };
            let changed = if peer_id == *self_id {
                let changed = *self_name != name;
                *self_name = name;
                changed
            } else {
                match peers.iter_mut().find(|peer| peer.info.id == peer_id) {
                    Some(peer) => {
                        let changed = peer.info.name != name;
                        peer.info.name = name;
                        changed
                    }
                    None => false,
                }
            };
            if changed {
                roster = Some(roster_of(*self_id, self_name, peers));
            }
        });
        let Some(roster) = roster else { return };
        self.peers.set(roster);
        self.broadcast_roster(None);
    }

    /// Every promotion and demotion in the room funnels through here, the host's own clicks
    /// included, so this is the one place the rules are checked: the requester must be an admin,
    /// and the target must be neither the requester nor the host. A request that fails is dropped
    /// rather than answered — the roster the host broadcasts is the only authority on who is an
    /// admin, so a requester acting on a stale one simply never sees the change it asked for.
    fn host_set_admin(self, requester: PeerId, target: PeerId, admin: bool) {
        let mut roster = None;
        self.state.update(|state| {
            let RoomState::Hosting { self_id, self_name, peers, .. } = state else { return };
            let requester_is_admin =
                requester == *self_id || peers.iter().any(|peer| peer.info.id == requester && peer.info.admin);
            if !requester_is_admin || target == requester || target == *self_id {
                tracing::debug!(requester = requester.0, target = target.0, "refusing an admin change");
                return;
            }
            let Some(peer) = peers.iter_mut().find(|peer| peer.info.id == target) else { return };
            if peer.info.admin == admin {
                return;
            }
            peer.info.admin = admin;
            roster = Some(roster_of(*self_id, self_name, peers));
        });
        let Some(roster) = roster else { return };
        self.peers.set(roster);
        self.broadcast_roster(None);
    }

    /// Deferred for the same reason as `on_host_message`: this runs from the closing link's own
    /// `on_close`, and clearing `pending`/`peers` here can drop that same link's last reference.
    fn on_host_peer_closed(self, peer_id: PeerId) {
        spawn_local(async move {
            let mut cleared = None;
            self.state.update(|state| {
                if let RoomState::Hosting { pending, .. } = state
                    && matches!(pending, Some((id, _)) if *id == peer_id)
                {
                    cleared = pending.take();
                }
            });
            if let Some((_, link)) = cleared {
                link.close();
                self.create_invite();
            } else {
                self.remove_peer(peer_id);
            }
        });
    }

    /// Explicitly closes the removed peer's link rather than just letting `Vec::retain` drop it:
    /// whatever's left of this peer's own event handlers (an `onclose` that hasn't fired yet, if
    /// this removal was itself triggered by a `Bye` on the same link) must be detached before the
    /// link disappears, or a later, separate event on it invokes an already-dropped `Closure`.
    /// Safe to call on an already-closing channel — closing twice is a no-op, not an error.
    fn remove_peer(self, peer_id: PeerId) {
        let mut removed_link = None;
        let mut roster = None;
        self.state.update(|state| {
            let RoomState::Hosting { self_id, self_name, peers, .. } = state else { return };
            if let Some(index) = peers.iter().position(|peer| peer.info.id == peer_id) {
                removed_link = Some(peers.remove(index).link);
                roster = Some(roster_of(*self_id, self_name, peers));
            }
        });
        if let Some(link) = removed_link {
            link.close();
        }
        let Some(roster) = roster else { return };
        self.peers.set(roster);
        self.broadcast_roster(None);
    }

    fn broadcast_roster(self, exclude: Option<PeerId>) {
        let links = self.state.with_untracked(|state| match state {
            RoomState::Hosting { peers, .. } => {
                peers.iter().filter(|peer| Some(peer.info.id) != exclude).map(|peer| peer.link.clone()).collect::<Vec<_>>()
            }
            _ => Vec::new(),
        });
        let message = Msg::<A>::Roster { peers: self.peers.get_untracked() };
        let Ok(json) = serde_json::to_string(&message) else { return };
        for link in links {
            let _ = link.send(&json);
        }
    }

    fn all_peer_links(self) -> Vec<Link> {
        self.state.with_untracked(|state| match state {
            RoomState::Hosting { peers, .. } => peers.iter().map(|peer| peer.link.clone()).collect(),
            _ => Vec::new(),
        })
    }

    fn host_link(self, peer_id: PeerId) -> Option<Link> {
        self.state.with_untracked(|state| match state {
            RoomState::Hosting { peers, .. } => peers.iter().find(|peer| peer.info.id == peer_id).map(|peer| peer.link.clone()),
            _ => None,
        })
    }

    /// Host-only in the UI that calls this; nothing here re-checks that, since there is no server
    /// to enforce it against a modified client anyway (see the room UI's own disclosure) — and
    /// unlike `host_set_admin`/`host_on_propose`, a non-host has no peer links to kick over in the
    /// first place, so there is nothing for a modified client to reach here even if it tried.
    pub fn kick(&self, peer_id: PeerId) {
        let this = *self;
        let Some(link) = this.host_link(peer_id) else { return };
        if let Ok(json) = serde_json::to_string(&Msg::<A>::Kick) {
            let _ = link.send(&json);
        }
        spawn_local(async move {
            // Closing right after `send` risks the `Kick` message never actually leaving the
            // local buffer — give the data channel a moment to flush it first. `remove_peer`
            // does the actual closing, re-reading the link fresh from `state` at that point.
            sleep_ms(100).await;
            this.remove_peer(peer_id);
        });
    }

    // ------------------------------------------------------- hosting: the two-phase-commit vote

    /// Where every `Propose` from a peer actually lands (the host's own proposals go straight to
    /// `host_enqueue`, since the host is always an admin). A peer whose admin flag was revoked a
    /// moment ago may still have one in flight, or a modified client may simply ignore the flag —
    /// either way the answer has to be an explicit `Abort` and not silence: nothing was enqueued,
    /// so no timeout is ever scheduled for it, and the proposer's `awaiting` slot only clears on a
    /// `Commit`/`Abort` naming its own `txn`. Dropping the message here would wedge that peer's
    /// `busy()` at true until it left the room.
    fn host_on_propose(self, peer_id: PeerId, txn: TxnId, request: A::Request) {
        let allowed = self.state.with_untracked(|state| match state {
            RoomState::Hosting { peers, .. } => peers.iter().any(|peer| peer.info.id == peer_id && peer.info.admin),
            _ => false,
        });
        if !allowed {
            let reason = AbortReason::Rejected { reason: "you are not an admin in this room".to_string() };
            let abort = Msg::<A>::Abort { txn, origin: peer_id, reason };
            if let (Some(link), Ok(json)) = (self.host_link(peer_id), serde_json::to_string(&abort)) {
                let _ = link.send(&json);
            }
            return;
        }
        self.host_enqueue(txn, peer_id, request, None);
    }

    /// Adds a proposal to the host's queue and, if nothing is already in flight, starts it. Used
    /// both for the host's own edits (`origin == self_id`, `on_settled` carries the caller's
    /// callback) and for an already-admin-checked `Propose` arriving from a peer (`on_settled` is
    /// `None` — that peer's own `awaiting` slot is what resolves their callback, once
    /// `Commit`/`Abort` reaches them).
    fn host_enqueue(self, txn: TxnId, origin: PeerId, request: A::Request, on_settled: Option<Settle>) {
        let mut idle = false;
        self.state.update(|state| {
            if let RoomState::Hosting { queue, in_flight, .. } = state {
                queue.push_back(QueuedProposal { txn, origin, request, on_settled });
                idle = in_flight.is_none();
            }
        });
        if idle {
            self.host_process_next();
        }
    }

    fn host_process_next(self) {
        let mut dequeued = None;
        self.state.update(|state| {
            if let RoomState::Hosting { queue, in_flight, .. } = state
                && in_flight.is_none()
            {
                dequeued = queue.pop_front();
            }
        });
        let Some(proposal) = dequeued else { return };
        self.host_begin_txn(proposal);
    }

    fn host_begin_txn(self, proposal: QueuedProposal<A>) {
        let QueuedProposal { txn, origin, request, on_settled } = proposal;
        let command = match self.app.sequence(request) {
            Ok(command) => command,
            Err(error) => {
                self.settle_locally(origin, on_settled, Err(error.to_string()));
                self.host_process_next();
                return;
            }
        };
        let before = self.app.state_hash();
        let own_vote = match self.app.dry_run(&command) {
            Ok(after) => VoteKind::Yes { before, after },
            Err(error) => VoteKind::No { before, reason: error.to_string() },
        };
        let self_id = self.self_id.get_untracked().unwrap_or(PeerId(0));
        let expected: Vec<PeerId> = self.state.with_untracked(|state| match state {
            RoomState::Hosting { peers, .. } => peers.iter().map(|peer| peer.info.id).collect(),
            _ => Vec::new(),
        });
        self.state.update(|state| {
            if let RoomState::Hosting { in_flight, .. } = state {
                *in_flight = Some(InFlight {
                    txn,
                    origin,
                    command: command.clone(),
                    before,
                    votes: vec![(self_id, own_vote)],
                    expected: expected.clone(),
                    on_settled,
                });
            }
        });
        if expected.is_empty() {
            self.host_try_decide(txn);
            return;
        }
        let prepare = Msg::<A>::Prepare { txn, origin, command };
        if let Ok(json) = serde_json::to_string(&prepare) {
            for link in self.all_peer_links() {
                let _ = link.send(&json);
            }
        }
        self.host_schedule_timeout(txn);
    }

    fn host_on_vote(self, peer_id: PeerId, txn: TxnId, vote: VoteKind) {
        let mut matched = false;
        self.state.update(|state| {
            if let RoomState::Hosting { in_flight: Some(flight), .. } = state
                && flight.txn == txn
                && !flight.votes.iter().any(|(voter, _)| *voter == peer_id)
            {
                flight.votes.push((peer_id, vote));
                matched = true;
            }
        });
        if matched {
            self.host_try_decide(txn);
        }
    }

    fn host_schedule_timeout(self, txn: TxnId) {
        spawn_local(async move {
            sleep_ms(VOTE_TIMEOUT_MS).await;
            let missing = self.state.with_untracked(|state| match state {
                RoomState::Hosting { in_flight: Some(flight), .. } if flight.txn == txn => Some(
                    flight.expected.iter().filter(|id| !flight.votes.iter().any(|(voter, _)| voter == *id)).copied().collect::<Vec<_>>(),
                ),
                _ => None,
            });
            let Some(missing) = missing else { return };
            if missing.is_empty() {
                return;
            }
            self.host_abort_txn(txn, AbortReason::Timeout);
            for peer_id in missing {
                self.remove_peer(peer_id);
            }
        });
    }

    fn host_try_decide(self, txn: TxnId) {
        enum Decision {
            NotReady,
            Commit(StateHash),
            Rejected(String),
            Diverged,
        }

        let decision = self.state.with_untracked(|state| {
            let RoomState::Hosting { in_flight: Some(flight), .. } = state else { return Decision::NotReady };
            if flight.txn != txn || !flight.expected.iter().all(|id| flight.votes.iter().any(|(voter, _)| voter == id)) {
                return Decision::NotReady;
            }
            if flight.votes.iter().any(|(_, vote)| vote_before(vote) != flight.before) {
                return Decision::Diverged;
            }
            if let Some((_, VoteKind::No { reason, .. })) = flight.votes.iter().find(|(_, vote)| matches!(vote, VoteKind::No { .. })) {
                return Decision::Rejected(reason.clone());
            }
            let afters: Vec<StateHash> =
                flight.votes.iter().map(|(_, vote)| match vote { VoteKind::Yes { after, .. } => *after, VoteKind::No { .. } => unreachable!("checked above: no No votes remain") }).collect();
            if afters.windows(2).all(|pair| pair[0] == pair[1]) {
                Decision::Commit(afters[0])
            } else {
                Decision::Diverged
            }
        });

        match decision {
            Decision::NotReady => {}
            Decision::Commit(after) => self.host_commit_txn(txn, after),
            Decision::Rejected(reason) => self.host_abort_txn(txn, AbortReason::Rejected { reason }),
            Decision::Diverged => {
                self.host_abort_txn(txn, AbortReason::Diverged);
                crate::ui::toast::error("Disconnected: peers disagreed about the battle state (this shouldn't happen)".to_string());
                self.leave();
            }
        }
    }

    fn host_commit_txn(self, txn: TxnId, after: StateHash) {
        let flight = self.take_in_flight(txn);
        let Some(flight) = flight else { return };
        if let Err(error) = self.app.commit(&flight.command) {
            // Shouldn't happen: every voter, including the host, just confirmed this exact
            // command would succeed from this exact state. If it does, there's nothing more
            // useful to do than log it — the vote already guarantees every other node applies
            // the same command, so the room stays internally consistent even if this is wrong.
            tracing::error!(%error, "host commit failed after a unanimous vote");
        }
        let commit = Msg::<A>::Commit { txn, origin: flight.origin, command: flight.command, after };
        if let Ok(json) = serde_json::to_string(&commit) {
            for link in self.all_peer_links() {
                let _ = link.send(&json);
            }
        }
        self.settle_locally(flight.origin, flight.on_settled, Ok(()));
        self.host_process_next();
    }

    fn host_abort_txn(self, txn: TxnId, reason: AbortReason) {
        let flight = self.take_in_flight(txn);
        let Some(flight) = flight else { return };
        let abort = Msg::<A>::Abort { txn, origin: flight.origin, reason: reason.clone() };
        if let Ok(json) = serde_json::to_string(&abort) {
            for link in self.all_peer_links() {
                let _ = link.send(&json);
            }
        }
        let message = match reason {
            AbortReason::Rejected { reason } => reason,
            AbortReason::Diverged => "the room disconnected because of a state mismatch".to_string(),
            AbortReason::Timeout => "a peer did not respond in time and was removed".to_string(),
        };
        self.settle_locally(flight.origin, flight.on_settled, Err(message));
        self.host_process_next();
    }

    fn take_in_flight(self, txn: TxnId) -> Option<InFlight<A>> {
        let mut taken = None;
        self.state.update(|state| {
            if let RoomState::Hosting { in_flight, .. } = state
                && matches!(in_flight, Some(flight) if flight.txn == txn)
            {
                taken = in_flight.take();
            }
        });
        taken
    }

    /// Fires `on_settled` only when this session is the one that proposed it — a request routed
    /// through `host_enqueue` on the host's own behalf carries a real callback; one that arrived
    /// as a `Propose` from a peer carries `None`, because that peer's own callback resolves from
    /// its `awaiting` slot once `Commit`/`Abort` reaches it instead.
    fn settle_locally(self, origin: PeerId, on_settled: Option<Settle>, result: Result<(), String>) {
        if self.self_id.get_untracked() != Some(origin) {
            return;
        }
        if let Some(on_settled) = on_settled {
            on_settled(result);
        }
    }

    // ---------------------------------------------------------------------------------- joined

    pub fn join(&self, offer_code: String, name: String) {
        let this = *self;
        this.mode.set(Mode::Joined);
        this.role.set(Role::Spectator);
        this.room_active.set(true);
        this.peers.set(Vec::new());
        this.answer_code.set(None);
        spawn_local(async move {
            let on_open = move |link: Link| {
                let hello = Msg::<A>::Hello { name: name.clone() };
                if let Ok(json) = serde_json::to_string(&hello) {
                    let _ = link.send(&json);
                }
            };
            let on_message = move |_link: Link, text: String| this.on_joined_message(text);
            let on_close = move || this.on_joined_disconnected();
            match Link::join(&offer_code, on_open, on_message, on_close).await {
                Ok((link, answer)) => {
                    // Real id arrives with `Welcome`; nothing observes this placeholder before then.
                    this.state.set(RoomState::Joined { self_id: PeerId(0), link, awaiting: None });
                    this.answer_code.set(Some(answer));
                    this.joined_schedule_connect_timeout();
                }
                Err(error) => {
                    report("join that room", error);
                    this.reset_to_solo();
                }
            }
        });
    }

    /// Gives up if `Welcome` never arrives after producing an answer — either the host never
    /// pasted it back, or ICE never connected. `self_id` only leaves `None` once `Welcome` is
    /// handled, and only `Mode::Joined` reaches this state at all, so both together are enough to
    /// tell "still waiting" apart from "already joined" or "already left".
    fn joined_schedule_connect_timeout(self) {
        spawn_local(async move {
            sleep_ms(JOIN_REPLY_TIMEOUT_MS).await;
            if self.mode.get_untracked() == Mode::Joined && self.self_id.get_untracked().is_none() {
                report_connect_timeout();
                self.reset_to_solo();
            }
        });
    }

    fn joined_link(self) -> Option<Link> {
        self.state.with_untracked(|state| match state {
            RoomState::Joined { link, .. } => Some(link.clone()),
            _ => None,
        })
    }

    fn joined_propose(self, request: A::Request, on_settled: Settle) {
        let Some(link) = self.joined_link() else {
            on_settled(Err("not connected to a room".to_string()));
            return;
        };
        let txn = self.next_txn_id();
        let mut superseded = None;
        self.state.update(|state| {
            if let RoomState::Joined { awaiting, .. } = state {
                superseded = awaiting.take();
                *awaiting = Some((txn, on_settled));
            }
        });
        // Shouldn't happen — the UI gates on `busy` — but a stuck caller is worse than a wrong
        // one: settle whatever was waiting before overwriting it, rather than leaking it forever.
        if let Some((_, previous)) = superseded {
            previous(Err("superseded by a newer proposal".to_string()));
        }
        let propose = Msg::<A>::Propose { txn, request };
        if let Ok(json) = serde_json::to_string(&propose) {
            let _ = link.send(&json);
        }
    }

    /// Deferred for the same reason as `on_host_message`: several branches here (`Kick`, a
    /// diverged `Commit`/`Abort`) end in `reset_to_solo`/`leave`, which can drop this same link's
    /// last reference from inside its own `on_message` callback.
    fn on_joined_message(self, text: String) {
        spawn_local(async move { self.handle_joined_message(text) });
    }

    fn handle_joined_message(self, text: String) {
        // `on_joined_message` defers this, so the room can be gone by the time it actually runs —
        // a `Kick` or a dropped connection can reset to `Mode::Solo` in between. Acting on a
        // message now would repopulate a room that no longer exists and, since `role` now follows
        // whatever roster last landed, could strand a solo battle as a read-only spectator of
        // nothing.
        if self.mode.get_untracked() != Mode::Joined {
            return;
        }
        let Ok(message) = serde_json::from_str::<Msg<A>>(&text) else {
            tracing::debug!("ignoring an undecodable message");
            return;
        };
        match message {
            Message::Welcome { you, host, roster, snapshot } => {
                if let Err(error) = self.app.restore(snapshot) {
                    tracing::error!(%error, "could not adopt the host's battle");
                    crate::ui::toast::error(format!("Could not adopt the shared battle: {error}"));
                }
                self.state.update(|state| {
                    if let RoomState::Joined { self_id, .. } = state {
                        *self_id = you;
                    }
                });
                self.self_id.set(Some(you));
                self.host_id.set(Some(host));
                self.peers.set(roster);
                self.sync_role_from_roster();
            }
            Message::Roster { peers } => {
                self.peers.set(peers);
                self.sync_role_from_roster();
            }
            Message::Kick => {
                crate::ui::toast::error("The host removed you from the room".to_string());
                self.reset_to_solo();
            }
            Message::Prepare { txn, command, .. } => self.joined_vote(txn, command),
            Message::Commit { txn, origin, command, after } => self.joined_commit(txn, origin, command, after),
            Message::Abort { txn, origin, reason } => self.joined_abort(txn, origin, reason),
            Message::Hello { .. }
            | Message::Bye
            | Message::Rename { .. }
            | Message::SetAdmin { .. }
            | Message::Propose { .. }
            | Message::Vote { .. } => {}
        }
    }

    /// The roster is the only authority on what this node may do, so `role` is re-read from it
    /// every time one lands rather than settled once at `Welcome`: a promotion or demotion reaches
    /// the affected peer as nothing more than an ordinary roster broadcast, and every control
    /// gated on `role` (via `read_only`) has to follow it without being told a second time.
    fn sync_role_from_roster(self) {
        let Some(self_id) = self.self_id.get_untracked() else { return };
        let admin = self.peers.with_untracked(|peers| peers.iter().any(|peer| peer.id == self_id && peer.admin));
        self.role.set(if admin { Role::Admin } else { Role::Spectator });
    }

    fn joined_vote(self, txn: TxnId, command: A::Command) {
        let Some(link) = self.joined_link() else { return };
        let before = self.app.state_hash();
        let vote = match self.app.dry_run(&command) {
            Ok(after) => VoteKind::Yes { before, after },
            Err(error) => VoteKind::No { before, reason: error.to_string() },
        };
        if let Ok(json) = serde_json::to_string(&Msg::<A>::Vote { txn, vote }) {
            let _ = link.send(&json);
        }
    }

    fn joined_commit(self, txn: TxnId, origin: PeerId, command: A::Command, after: StateHash) {
        if let Err(error) = self.app.commit(&command) {
            tracing::error!(%error, "could not apply a change the room agreed to");
        }
        if self.app.state_hash() != after {
            crate::ui::toast::error("Disconnected: this browser's battle no longer matches the room's".to_string());
            self.settle_joined(txn, origin, Err("state mismatch after commit".to_string()));
            self.leave();
            return;
        }
        self.settle_joined(txn, origin, Ok(()));
    }

    fn joined_abort(self, txn: TxnId, origin: PeerId, reason: AbortReason) {
        let message = match &reason {
            AbortReason::Rejected { reason } => reason.clone(),
            AbortReason::Diverged => "the room disconnected because of a state mismatch".to_string(),
            AbortReason::Timeout => "timed out waiting for the room to agree".to_string(),
        };
        self.settle_joined(txn, origin, Err(message));
        if reason == AbortReason::Diverged {
            crate::ui::toast::error("Disconnected: peers disagreed about the battle state (this shouldn't happen)".to_string());
            self.leave();
        }
    }

    fn settle_joined(self, txn: TxnId, origin: PeerId, result: Result<(), String>) {
        if self.self_id.get_untracked() != Some(origin) {
            return;
        }
        let mut settle = None;
        self.state.update(|state| {
            if let RoomState::Joined { awaiting, .. } = state
                && matches!(awaiting, Some((t, _)) if *t == txn)
            {
                settle = awaiting.take().map(|(_, settle)| settle);
            }
        });
        if let Some(settle) = settle {
            settle(result);
        }
    }

    /// Deferred for the same reason as `on_host_message`: this runs from the closing link's own
    /// `on_close`, and `reset_to_solo` can drop that same link's last reference.
    fn on_joined_disconnected(self) {
        spawn_local(async move {
            if self.mode.get_untracked() == Mode::Joined {
                crate::ui::toast::error("Lost the connection to the host".to_string());
                self.reset_to_solo();
            }
        });
    }

    // ------------------------------------------------------------------------ shared teardown

    pub fn leave(&self) {
        let this = *self;
        if matches!(this.mode.get_untracked(), Mode::Solo) {
            return;
        }
        // `Bye` first, over the still-open links; `reset_to_solo` closes them right after.
        let links = this.state.with_untracked(|state| match state {
            RoomState::Hosting { peers, pending, .. } => {
                let mut links: Vec<Link> = peers.iter().map(|peer| peer.link.clone()).collect();
                if let Some((_, link)) = pending {
                    links.push(link.clone());
                }
                links
            }
            RoomState::Joined { link, .. } => vec![link.clone()],
            RoomState::Solo => Vec::new(),
        });
        let bye = serde_json::to_string(&Msg::<A>::Bye).ok();
        for link in &links {
            if let Some(json) = &bye {
                let _ = link.send(json);
            }
        }
        this.reset_to_solo();
    }

    /// Closes whatever link(s) `state` still holds before dropping them, and settles any
    /// outstanding proposal as failed rather than leaving its caller waiting forever.
    /// `leave()` already closes its links up front, but `on_joined_disconnected` and the `Kick`
    /// handler call this directly while a link they didn't close is still live in `state`, and
    /// letting `Vec`/`Option` drop it unclosed leaves a stray event with nothing to catch it
    /// later — see `remove_peer`'s doc.
    fn reset_to_solo(self) {
        let self_id = self.self_id.get_untracked();
        let mut links = Vec::new();
        let mut stranded = Vec::new();
        self.state.update(|state| match state {
            RoomState::Hosting { peers, pending, queue, in_flight, .. } => {
                links.extend(peers.drain(..).map(|peer| peer.link));
                if let Some((_, link)) = pending.take() {
                    links.push(link);
                }
                for proposal in queue.drain(..) {
                    if Some(proposal.origin) == self_id {
                        stranded.extend(proposal.on_settled);
                    }
                }
                if let Some(flight) = in_flight.take()
                    && Some(flight.origin) == self_id
                {
                    stranded.extend(flight.on_settled);
                }
            }
            RoomState::Joined { link, awaiting, .. } => {
                links.push(link.clone());
                if let Some((_, on_settled)) = awaiting.take() {
                    stranded.push(on_settled);
                }
            }
            RoomState::Solo => {}
        });
        for link in &links {
            link.close();
        }
        // Whatever this node itself was waiting on will never hear back now.
        for on_settled in stranded {
            on_settled(Err("disconnected before this could be agreed on".to_string()));
        }
        self.state.set(RoomState::Solo);
        self.mode.set(Mode::Solo);
        self.role.set(Role::Host);
        self.self_id.set(None);
        self.host_id.set(None);
        self.peers.set(Vec::new());
        self.invite.set(None);
        self.answer_code.set(None);
        self.room_active.set(false);
    }
}
