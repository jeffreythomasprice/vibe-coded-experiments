//! The websocket wire protocol between a client and the server's `/ws` endpoint. The server is
//! the sole authority here: it owns every room's `BattleLog`, validates every move, and broadcasts
//! the result — a client never reconciles two copies of anything, it just replaces its state
//! wholesale with whatever the server last sent.

use crate::battle::BattleLog;
use crate::protocol::BattleRequest;
use serde::{Deserialize, Serialize};

/// Identifies one live websocket connection, minted by the server when the socket is accepted.
/// This is never generated client-side — the server is the only node that hands these out, the
/// same way it is the only node that mints combatant and marker ids into a room's log.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConnectionId(pub String);

/// Correlates a reply with the request that caused it — minted by the client, meaningful only to
/// the connection that sent it. A message the server sends unprompted (a `Members` update caused
/// by someone else's action, a `Left { reason: Kicked }`) carries no `reply_to` at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RequestId(pub u64);

/// One entry in a room's roster, as seen by anyone in it — including the host and yourself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    pub id: ConnectionId,
    pub name: String,
    pub can_write: bool,
    /// The connection that created this room. Never demoted, never kicked — see `ClientMessage`'s
    /// doc comment on `SetWritable`/`Kick`. Cleared for good once that connection leaves; nobody
    /// else in the room ever becomes host in its place.
    pub is_host: bool,
}

/// What a client sends. Every message carries the caller's access token — there is no separate
/// handshake, and no assumption that a token checked out once stays valid; the server re-checks it
/// on every single message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientEnvelope {
    pub id: RequestId,
    pub token: String,
    pub message: ClientMessage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMessage {
    /// Creates a room, seeded with the caller's own current battle, and joins it as host under
    /// `name`. `everyone_writes` sets what a *future* joiner starts as — not retroactive, and
    /// irrelevant to the host, who can always write. Fails with `ProtocolError::RoomExists` if the
    /// name is already taken by a room that hasn't expired.
    Create { room: String, name: String, everyone_writes: bool, log: BattleLog },
    /// Joins an existing room by name, under `name`. `can_write` is granted from the room's
    /// current `everyone_writes` flag alone.
    Join { room: String, name: String },
    /// Leaves whatever room this connection is in. A no-op reply, not an error, if it isn't in
    /// one.
    Leave,
    /// Sets this connection's own display name. The one action a readonly member may take beyond
    /// watching and leaving.
    Rename { name: String },
    /// Grants or revokes another member's write access. Readwrite-only, and never against the
    /// caller themselves or the room's host — both are `ProtocolError::NotAllowed`.
    SetWritable { member: ConnectionId, can_write: bool },
    /// Changes what a *future* joiner starts as. Readwrite-only.
    SetEveryoneWrites { everyone_writes: bool },
    /// Disconnects another member. Readwrite-only, and never against the caller themselves or the
    /// room's host.
    Kick { member: ConnectionId },
    /// A move against the shared battle. Readwrite-only; a readonly member's attempt is
    /// `ProtocolError::ReadOnly`.
    Request(BattleRequest),
    /// Asks the server to resend the room's current membership and battle state, in case a
    /// broadcast was ever missed. Available to readonly members too — it changes nothing, it only
    /// asks.
    Resync,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerEnvelope {
    /// `Some` only when this is a direct reply to one particular `ClientEnvelope`; a broadcast
    /// caused by someone else's action carries `None`.
    pub reply_to: Option<RequestId>,
    pub message: ServerMessage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMessage {
    /// Reply to a successful `Create` or `Join`: everything the client needs to render the room
    /// from scratch, including its own id and write permission.
    Joined {
        room: String,
        you: ConnectionId,
        can_write: bool,
        everyone_writes: bool,
        members: Vec<Member>,
        version: u64,
        log: BattleLog,
    },
    /// Sent to every member of a room whenever who's in it, their names, or their write
    /// permission changes. Includes the actor's own connection, so every client's view of the
    /// roster comes from the same broadcast rather than an optimistic local update.
    Members { members: Vec<Member>, everyone_writes: bool },
    /// Sent to every member of a room after a `Request` changes its battle. The whole log, not a
    /// diff — see the module doc.
    State { room: String, version: u64, log: BattleLog },
    /// This connection is no longer in the room it was in.
    Left { reason: LeaveReason },
    Error { error: ProtocolError },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LeaveReason {
    /// This connection sent `Leave` itself.
    Requested,
    /// A readwrite member kicked this connection.
    Kicked,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
pub enum ProtocolError {
    #[error("missing or unknown access code")]
    Unauthorized,
    #[error("a room with that name already exists")]
    RoomExists,
    #[error("no room with that name exists")]
    NoSuchRoom,
    #[error("you are not in a room")]
    NotInRoom,
    #[error("you are already in a room \u{2014} leave it first")]
    AlreadyInRoom,
    #[error("you do not have write access in this room")]
    ReadOnly,
    #[error("that action is not allowed")]
    NotAllowed,
    #[error("{0}")]
    BadRoomName(String),
    #[error("{0}")]
    IllegalMove(String),
    #[error("that battle is too large to store")]
    TooLarge,
    #[error("internal error")]
    Internal,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelopes_round_trip_through_json() {
        let envelope = ClientEnvelope {
            id: RequestId(1),
            token: "tok".to_string(),
            message: ClientMessage::Join { room: "test".to_string(), name: "Alice".to_string() },
        };
        let json = serde_json::to_string(&envelope).unwrap();
        let decoded: ClientEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, envelope.id);
    }

    #[test]
    fn protocol_errors_carry_their_own_message() {
        let error = ProtocolError::IllegalMove("nothing to undo".to_string());
        assert_eq!(error.to_string(), "nothing to undo");
    }
}
