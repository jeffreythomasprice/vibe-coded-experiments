//! Wire-level types and pure logic shared between the client and server halves of multiplayer:
//! nothing here knows about WebRTC, HTTP, or any other transport.

mod command;
mod hash;
mod peer;

pub use command::{apply_command, BattleCommand, BattleRequest, BattleSyncError};
pub use hash::{hash_of, StateHash};
pub use peer::{PeerId, PeerInfo};
