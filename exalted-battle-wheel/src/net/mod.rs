//! The generic half of multiplayer: a seam (`Replicated`) that lets a `Session` sequence, check,
//! apply, and snapshot commands without knowing what any of them mean. Nothing in this module may
//! know about battles, ticks, or combatants — see `battle_net.rs` for the app-specific glue.

mod code;
mod error;
mod hash;
mod message;
mod rtc;
mod sdp;
mod session;

pub use error::RoomError;
pub use hash::{hash_of, StateHash};
pub use message::PeerInfo;
pub use rtc::{set_stun_servers, Link};
pub use session::{set_root_owner, Mode, Role, Session};

use serde::de::DeserializeOwned;
use serde::Serialize;

/// Identifies one connection in a room — the host included. Random rather than sequential, since
/// there is no server to hand out sequential ones: generated from `crypto.getRandomValues`, so
/// collisions are not a practical concern at this scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct PeerId(pub u64);

/// Identifies one proposed command, scoped to whoever proposed it — a peer's own counter, not a
/// globally unique id. The host correlates a `Propose` by (the link it arrived on, this id), and
/// every broadcast afterward carries the proposer's `PeerId` alongside it so recipients can tell
/// "this is mine" from "this is someone else's" without needing the id itself to be unique room-wide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct TxnId(pub u64);

/// The seam between the networking layer and an application's own state. A `Session<A>` drives
/// these operations without ever inspecting `Request`, `Command`, or `Snapshot` — so a command's
/// meaning, and the rules for accepting or rejecting it, live entirely in the implementation.
pub trait Replicated: Copy + 'static {
    /// What a would-be admin asks for. May carry placeholder ids for anything it mints — only
    /// `sequence` (running on whichever node is authoritative) turns those into real ones.
    type Request: Serialize + DeserializeOwned + Clone + 'static;
    /// The concrete, fully-resolved instruction every node applies identically. Unlike `Request`,
    /// this never carries a placeholder.
    type Command: Serialize + DeserializeOwned + Clone + 'static;
    /// The whole of the app's state, sent wholesale to a joining node.
    type Snapshot: Serialize + DeserializeOwned + 'static;
    type Error: std::error::Error + Clone + PartialEq + 'static;

    /// Turns a proposal into the concrete command every node will apply. Only ever called once
    /// per request, by whichever node is authoritative for the room, after every earlier command
    /// has already committed — so anything a `Command` mints here is a deterministic function of
    /// the sequence it was assigned.
    fn sequence(&self, request: Self::Request) -> Result<Self::Command, Self::Error>;

    /// The two-phase-commit "prepare": checks whether `command` would succeed, and what state
    /// hash applying it would produce, without mutating anything. Every node runs this to vote on
    /// a proposed command before anyone commits it.
    fn dry_run(&self, command: &Self::Command) -> Result<StateHash, Self::Error>;

    /// Applies `command`, mutating local state. Must succeed for any `Command` that just won a
    /// unanimous vote from an identical `before` state; a mismatch here (a hash that doesn't match
    /// what `dry_run` promised) is a local divergence, not an ordinary rejection.
    fn commit(&self, command: &Self::Command) -> Result<(), Self::Error>;

    fn state_hash(&self) -> StateHash;
    fn snapshot(&self) -> Self::Snapshot;
    fn restore(&self, snapshot: Self::Snapshot) -> Result<(), Self::Error>;
}
