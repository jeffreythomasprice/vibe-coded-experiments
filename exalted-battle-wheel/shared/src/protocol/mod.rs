//! Wire-level types and pure logic shared between the client and server halves of multiplayer:
//! nothing here knows about WebSocket framing, HTTP, or any other transport detail.

mod command;
mod name;
mod room;

pub use command::{BattleCommand, BattleRequest, BattleSyncError, apply_command};
pub use name::{MAX_NAME_LEN, MAX_ROOM_NAME_LEN, RoomNameError, room_key, sanitize_name};
pub use room::{
    ClientEnvelope, ClientMessage, ConnectionId, LeaveReason, Member, MemberName, ProtocolError, RequestId, RoomName, ServerEnvelope,
    ServerMessage, SessionRejection,
};
