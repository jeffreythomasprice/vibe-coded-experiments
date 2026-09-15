use serde::{Deserialize, Serialize};

/// Identifies one connection in a room — the host included. Random rather than sequential, since
/// there is no server to hand out sequential ones: generated from `crypto.getRandomValues`, so
/// collisions are not a practical concern at this scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PeerId(pub u64);

/// One entry in the room's roster, as seen by anyone in it — including the host and yourself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerInfo {
    pub id: PeerId,
    pub name: String,
    pub admin: bool,
}
