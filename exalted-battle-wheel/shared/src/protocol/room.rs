//! The websocket wire protocol between a client and the server's `/ws` endpoint. The server is
//! the sole authority here: it owns every room's `BattleLog`, validates every move, and broadcasts
//! the result — a client never reconciles two copies of anything, it just replaces its state
//! wholesale with whatever the server last sent.
//!
//! Every type below except `SessionRejection` is defined in `shared/schemas/ws.json`; see
//! `shared/schemas/README.md`.

pub use crate::generated::{
    ClientEnvelope, ClientMessage, ConnectionId, LeaveReason, Member, MemberName, ProtocolError, RequestId,
    RoomName, ServerEnvelope, ServerMessage,
};

/// Why a `Join`'s `session` token was refused — split out from `ProtocolError` so the client can
/// decide per-reason whether the room is still worth retrying without the token. Hand-written
/// rather than generated, since typify's auto `Display` would collide with these `thiserror`
/// messages (see `shared/schemas/README.md`).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, serde::Serialize, serde::Deserialize)]
pub enum SessionRejection {
    #[error("that room session could not be read")]
    Malformed,
    #[error("that room session has expired")]
    Expired,
    #[error("that room session is for a different room")]
    WrongRoom,
}

/// Same reasoning as `SessionRejection` above: typify has no way to attach a `thiserror`-style
/// message to a generated variant, so this is hand-written to keep the exact strings the client
/// renders into toasts (`battle_net.rs`, `api/error.rs`).
impl std::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProtocolError::Unauthorized => write!(f, "missing or unknown access code"),
            ProtocolError::RoomExists => write!(f, "a room with that name already exists"),
            ProtocolError::NoSuchRoom => write!(f, "no room with that name exists"),
            ProtocolError::NotInRoom => write!(f, "you are not in a room"),
            ProtocolError::AlreadyInRoom => write!(f, "you are already in a room \u{2014} leave it first"),
            ProtocolError::ReadOnly => write!(f, "you do not have write access in this room"),
            ProtocolError::NotAllowed => write!(f, "that action is not allowed"),
            ProtocolError::BadRoomName(message) => write!(f, "{message}"),
            ProtocolError::IllegalMove(message) => write!(f, "{message}"),
            ProtocolError::InvalidSession(rejection) => write!(f, "{rejection}"),
            ProtocolError::TooLarge => write!(f, "that battle is too large to store"),
            ProtocolError::Internal => write!(f, "internal error"),
        }
    }
}

impl std::error::Error for ProtocolError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelopes_round_trip_through_json() {
        let envelope = ClientEnvelope {
            id: RequestId(1),
            token: "tok".to_string(),
            message: ClientMessage::Join {
                room: "test".try_into().unwrap(),
                name: "Alice".try_into().unwrap(),
                session: None,
            },
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
