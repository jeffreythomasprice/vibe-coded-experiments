//! Data types shared between the `server` and the Leptos `client`.
//!
//! Everything here is `serde`-(de)serializable so the same structs describe
//! both the HTTP request/response bodies and the WebSocket frames. WebSocket
//! messages are serialized as JSON with an internal `"type"` tag.

use serde::{Deserialize, Serialize};

/// Identifier for a game room / table.
pub type RoomId = String;
/// Identifier for a connected client (assigned by the server on connect).
pub type ClientId = String;

// ---------------------------------------------------------------------------
// HTTP
// ---------------------------------------------------------------------------

/// Response body for `GET /health`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
}

/// Response body for `GET /api/version`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionResponse {
    pub name: String,
    pub version: String,
}

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

/// A chat line shown in the table's chat log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub from: ClientId,
    pub text: String,
}

/// A point in map/scene coordinates.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

/// A token (mini) being moved on the map.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenMove {
    pub token_id: String,
    pub pos: Position,
}

// ---------------------------------------------------------------------------
// WebSocket: client -> server
// ---------------------------------------------------------------------------

/// Messages sent from a client up to the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// Join a room with a display name.
    Join { room: RoomId, name: String },
    /// Send a chat line to the room.
    Chat { text: String },
    /// Move a token on the map.
    MoveToken(TokenMove),
    /// Keepalive.
    Ping,
}

// ---------------------------------------------------------------------------
// WebSocket: server -> client
// ---------------------------------------------------------------------------

/// Messages broadcast from the server down to clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Sent once on connect with the client's assigned id.
    Welcome { client_id: ClientId },
    /// A chat line to display.
    Chat(ChatMessage),
    /// A token moved on the map.
    TokenMoved(TokenMove),
    /// Current presence list for the room.
    Presence { clients: Vec<ClientId> },
    /// Keepalive reply.
    Pong,
    /// An error to surface to the user.
    Error { message: String },
}
