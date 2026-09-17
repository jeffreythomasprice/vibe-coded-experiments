//! Wire types for the server's `GET /rooms` HTTP endpoint — a plain snapshot listing, crossing an
//! ordinary HTTP connection like `access`'s types. Unlike `protocol::room`, which is the live
//! websocket protocol, nothing here is ever sent over a room's own connection. Defined in
//! `shared/schemas/rooms.json`; see `shared/schemas/README.md`.

pub use crate::generated::{RoomList, RoomSummary};
