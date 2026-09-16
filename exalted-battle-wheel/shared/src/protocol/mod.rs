//! Wire-level types and pure logic shared between the client and server halves of multiplayer:
//! nothing here knows about WebSocket framing, HTTP, or any other transport detail.

mod command;
mod name;
mod room;

pub use command::{apply_command, BattleCommand, BattleRequest, BattleSyncError};
pub use name::{room_key, sanitize_name, RoomNameError, MAX_NAME_LEN, MAX_ROOM_NAME_LEN};
pub use room::{
    ClientEnvelope, ClientMessage, ConnectionId, LeaveReason, Member, ProtocolError, RequestId, ServerEnvelope,
    ServerMessage,
};
