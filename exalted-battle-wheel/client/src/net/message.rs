use crate::net::TxnId;
use serde::{Deserialize, Serialize};
use shared::protocol::{PeerId, PeerInfo, StateHash};

/// A vote on a proposed command, cast against the state as it stood *before* applying it — the
/// host's decision hinges on comparing `before` across every vote, not just tallying yes/no (see
/// `Session`'s vote-counting): if any two voters' `before` disagree, that's a bug worth
/// disconnecting over; if they agree and someone still says no, that's just an ordinary rejection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VoteKind {
    Yes { before: StateHash, after: StateHash },
    No { before: StateHash, reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AbortReason {
    /// The proposal never went to a vote at all, or every voter saw the same `before` and still
    /// rejected it (e.g. a `Seek` past a truncated redo tail). The former covers the host refusing
    /// a non-admin's `Propose` outright. Either way the room stays healthy; only the proposer hears
    /// about it.
    Rejected { reason: String },
    /// Voters disagreed about `before`, or all said yes but disagreed about `after` — the same
    /// command produced different results on different nodes, which should be impossible if
    /// everyone is replaying the same log. Something is actually wrong, so the whole room tears
    /// down rather than risk silently forking.
    Diverged,
    /// A voter never replied. The room stays healthy for everyone else; the silent peer is
    /// dropped.
    Timeout,
}

/// What crosses the data channel. Generic over the app's request/command/snapshot types only —
/// nothing here is specific to battles, and every peer speaks the same enum regardless of which
/// node is host.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message<Request, Command, Snapshot> {
    /// Peer -> host, sent the moment the channel opens. There is no protocol handshake before
    /// this — the WebRTC connection itself already proves the two sides exchanged a valid
    /// offer/answer, so a first message is the only "are you really there" check needed.
    Hello { name: String },
    /// Host -> the peer that just said `Hello`. Carries the whole battle so the peer can adopt it
    /// wholesale, and the roster as it stands now that this peer counts as a member. Admin status
    /// is read from this peer's own entry in `roster` (already present — `on_hello` adds it before
    /// building this message), not carried separately, since a separately-carried bit would become
    /// a second, staler source of truth the moment anyone is promoted or demoted.
    Welcome { you: PeerId, host: PeerId, roster: Vec<PeerInfo>, snapshot: Snapshot },
    /// Host -> everyone, whenever membership or a peer's name or admin flag changes.
    Roster { peers: Vec<PeerInfo> },
    /// Host -> one peer, over that peer's own link. Being kicked is closing this specific
    /// connection, so there is nothing else to identify — receiving this at all is the target.
    Kick,
    /// Peer -> host, sent on a deliberate, voluntary disconnect (closing the room UI) so the host
    /// can drop them immediately rather than waiting on the channel to time out.
    Bye,
    /// Peer -> host: "call me this from now on." The host owns every name in the room, so this is
    /// a request, not an announcement — nothing changes anywhere, including for the sender, until
    /// the host's own `Roster` broadcast carries the new name back around.
    Rename { name: String },
    /// Peer -> host: "this peer should (or should not) be an admin." Only an admin may ask, and
    /// never about themselves or the host; the host re-checks all three regardless of what the
    /// asker's own roster currently says, since it may be a broadcast out of date. A request that
    /// fails any check is dropped without a reply — the roster the host broadcasts is the only
    /// authority on who is an admin, so a rejected request simply produces no roster change, which
    /// is already the right outcome.
    SetAdmin { peer: PeerId, admin: bool },

    /// Peer -> host: "I'd like to make this change." `txn` is only unique from this peer's own
    /// point of view — the host correlates it with whichever link it arrived on, not with the id
    /// alone, so two different peers' `txn`s may collide without confusion.
    Propose { txn: TxnId, request: Request },
    /// Host -> everyone (host included, applied locally without a round trip to itself): the
    /// concrete command being voted on, and who proposed it. Every node runs `dry_run` on receipt
    /// and replies with a `Vote` — including the host, over no wire at all.
    Prepare { txn: TxnId, origin: PeerId, command: Command },
    /// Any node -> host, in answer to a `Prepare`.
    Vote { txn: TxnId, vote: VoteKind },
    /// Host -> everyone: the vote passed unanimously with agreeing hashes. Carries `command` again
    /// (rather than making recipients cache it from `Prepare`) so applying a commit never depends
    /// on state kept between messages, and `after` so a receiving node can confirm its own result
    /// matches what everyone agreed to.
    Commit { txn: TxnId, origin: PeerId, command: Command, after: StateHash },
    /// Host -> everyone: the proposal did not go through.
    Abort { txn: TxnId, origin: PeerId, reason: AbortReason },
}
