//! Wire types for the server's `GET /rooms` HTTP endpoint — a plain snapshot listing, crossing an
//! ordinary HTTP connection like `access`'s types. Unlike `protocol::room`, which is the live
//! websocket protocol, nothing here is ever sent over a room's own connection.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomSummary {
    pub display_name: String,
    pub member_count: usize,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomList {
    pub rooms: Vec<RoomSummary>,
}
