//! The app-specific half of multiplayer: teaches the generic `net::Session` how to sequence and
//! apply changes to a `BattleLog`, and exposes `Battles` as the one facade every UI call site
//! uses instead of touching the log directly.

use crate::net::{hash_of, Mode, PeerId, PeerInfo, Replicated, Role, Session, StateHash};
use exalted_battle_wheel::battle::{BattleError, BattleEvent, BattleLog};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

/// What a caller asks for. `PushMinting` carries an event with placeholder ids (`CombatantId(0)`,
/// `MarkerId(0)`, ...) for whatever it mints — only `BattleApp::sequence` turns those into real
/// ones, by stamping them from whichever log is authoritative for the room. Plain `Push` is for
/// events that mint nothing, or that intentionally carry ids minted earlier by another event
/// (`ReviseCombatant`'s `InSequence` case clones an existing sequence's effect ids).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BattleRequest {
    Push(BattleEvent),
    PushMinting(BattleEvent),
    Undo,
    Redo,
    Seek(usize),
    Reset,
}

/// The concrete instruction every node applies identically — never a placeholder id in sight.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BattleCommand {
    Push(BattleEvent),
    Undo,
    Redo,
    Seek(usize),
    Reset,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BattleSyncError {
    #[error(transparent)]
    Battle(#[from] BattleError),
}

fn apply_command(log: &mut BattleLog, command: &BattleCommand) -> Result<(), BattleError> {
    match command {
        BattleCommand::Push(event) => log.push(event.clone()),
        BattleCommand::Undo => log.undo(),
        BattleCommand::Redo => log.redo(),
        BattleCommand::Seek(cursor) => log.seek(*cursor),
        BattleCommand::Reset => {
            *log = BattleLog::new();
            Ok(())
        }
    }
}

#[derive(Clone, Copy)]
struct BattleApp {
    log: RwSignal<BattleLog>,
}

impl Replicated for BattleApp {
    type Request = BattleRequest;
    type Command = BattleCommand;
    type Snapshot = BattleLog;
    type Error = BattleSyncError;

    fn sequence(&self, request: BattleRequest) -> Result<BattleCommand, BattleSyncError> {
        Ok(match request {
            BattleRequest::Push(event) => BattleCommand::Push(event),
            BattleRequest::PushMinting(event) => BattleCommand::Push(self.log.read_untracked().restamp(event)),
            BattleRequest::Undo => BattleCommand::Undo,
            BattleRequest::Redo => BattleCommand::Redo,
            BattleRequest::Seek(cursor) => BattleCommand::Seek(cursor),
            BattleRequest::Reset => BattleCommand::Reset,
        })
    }

    fn dry_run(&self, command: &BattleCommand) -> Result<StateHash, BattleSyncError> {
        let mut probe = self.log.get_untracked();
        apply_command(&mut probe, command)?;
        Ok(hash_of(&probe))
    }

    fn commit(&self, command: &BattleCommand) -> Result<(), BattleSyncError> {
        let mut result = Ok(());
        self.log.update(|log| result = apply_command(log, command));
        result.map_err(BattleSyncError::from)
    }

    fn state_hash(&self) -> StateHash {
        hash_of(&self.log.get_untracked())
    }

    fn snapshot(&self) -> BattleLog {
        self.log.get_untracked()
    }

    fn restore(&self, snapshot: BattleLog) -> Result<(), BattleSyncError> {
        // `snapshot` decoded from JSON on the way in — via `BattleLog`'s own
        // `#[serde(try_from = "RestoredLog")]` — so by the time it's a `BattleLog` value at all it
        // has already replayed clean and had its id counters checked. Nothing left to reject here.
        self.log.set(snapshot);
        Ok(())
    }
}

/// The read-only view every UI component reads the battle log through. Context is keyed by type
/// alone, so replacing the writable `RwSignal<BattleLog>` that used to be provided with this
/// instead makes bypassing `Battles` a compile error, not just a convention.
pub type BattleView = ReadSignal<BattleLog>;

/// The one facade every UI call site uses to change the battle. Replaces direct
/// `log.update(|log| log.push(..))` calls so that a future networked room can intercept every
/// mutation at a single chokepoint; today (`Session` is solo-only) it settles synchronously and
/// behaves exactly like the direct calls it replaces.
#[derive(Clone, Copy)]
pub struct Battles {
    session: Session<BattleApp>,
}

impl Battles {
    /// `room_active` must be the same signal already passed to the battle log's
    /// `Persisted::new_gated` — see `Session::new`'s doc comment for why it's threaded in rather
    /// than created here.
    pub fn new(log: RwSignal<BattleLog>, room_active: RwSignal<bool>) -> Self {
        Self { session: Session::new(BattleApp { log }, room_active) }
    }

    pub fn push(&self, event: BattleEvent) {
        self.propose(BattleRequest::Push(event), "push event");
    }

    pub fn push_minting(&self, event: BattleEvent) {
        self.propose(BattleRequest::PushMinting(event), "push event");
    }

    pub fn push_with(&self, event: BattleEvent, on_settled: impl FnOnce(Result<(), String>) + 'static) {
        self.propose_with(BattleRequest::Push(event), on_settled);
    }

    pub fn push_minting_with(&self, event: BattleEvent, on_settled: impl FnOnce(Result<(), String>) + 'static) {
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

    pub fn mode(&self) -> Signal<Mode> {
        self.session.mode()
    }

    pub fn role(&self) -> Signal<Role> {
        self.session.role()
    }

    pub fn self_id(&self) -> Signal<Option<PeerId>> {
        self.session.self_id()
    }

    pub fn peers(&self) -> Signal<Vec<PeerInfo>> {
        self.session.peers()
    }

    pub fn invite(&self) -> Signal<Option<String>> {
        self.session.invite()
    }

    pub fn answer_code(&self) -> Signal<Option<String>> {
        self.session.answer_code()
    }

    /// Whether a proposal is currently awaiting agreement — while a room exists, editing the
    /// battle is a network round trip, not a local call, and the UI should hold off on starting a
    /// second change until the first has settled.
    pub fn busy(&self) -> Signal<bool> {
        self.session.busy()
    }

    pub fn host(&self, name: String, everyone_admin: bool) {
        self.session.host(name, everyone_admin);
    }

    pub fn accept_answer(&self, code: String) {
        self.session.accept_answer(code);
    }

    pub fn join(&self, offer_code: String, name: String) {
        self.session.join(offer_code, name);
    }

    pub fn kick(&self, peer: PeerId) {
        self.session.kick(peer);
    }

    pub fn leave(&self) {
        self.session.leave();
    }

    fn propose(&self, request: BattleRequest, action: &'static str) {
        self.propose_with(request, move |result| {
            if let Err(error) = result {
                tracing::error!(%error, "could not {action}");
                crate::ui::toast::error(format!("Could not {action}: {error}"));
            }
        });
    }

    /// For `undo`/`redo`/`seek`: their buttons are already disabled when there's nothing to do, so
    /// a rejection here only ever comes from a harmless race (another peer's command landed
    /// first) rather than a user action gone wrong, and isn't worth a toast.
    fn propose_quiet(&self, request: BattleRequest) {
        self.propose_with(request, |result| {
            if let Err(error) = result {
                tracing::debug!(%error, "no-op");
            }
        });
    }

    fn propose_with(&self, request: BattleRequest, on_settled: impl FnOnce(Result<(), String>) + 'static) {
        self.session.propose(request, on_settled);
    }
}
