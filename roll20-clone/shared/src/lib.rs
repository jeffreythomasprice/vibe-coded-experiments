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
/// Identifier for a map.
pub type MapId = String;
/// Identifier for a shape.
pub type ShapeId = String;
/// Identifier for a group.
pub type GroupId = String;

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
// Maps
// ---------------------------------------------------------------------------

/// A geometric primitive. Coordinates are in *grid units* (e.g. a rect at
/// `x = 1, y = 1` spanning `w = 2, h = 3` covers squares (1,1)..(3,4)).
///
/// Internally tagged so additional primitives (circle, polygon, ...) can be
/// added later without breaking existing serialized data.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "shape", rename_all = "snake_case")]
pub enum Geometry {
    Rect { x: f64, y: f64, w: f64, h: f64 },
}

/// Visual properties shared by shapes and groups. Every editable field lives
/// here so the client's property inspector can edit shapes and groups uniformly.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Style {
    /// Outline color as a CSS color string, e.g. `"#000000"`.
    pub line_color: String,
    /// Outline width in pixels at zoom 1.
    pub line_width: f64,
    /// Fill color as a CSS color string (may use `rgba(...)` for alpha).
    pub background_color: String,
}

/// A single geometric primitive with its own style.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Shape {
    pub id: ShapeId,
    pub geometry: Geometry,
    pub style: Style,
}

/// A boolean operator combining two sub-trees.
///
/// `Subtract` is order-sensitive: `left SUBTRACT right` removes `right` from
/// `left`. `Union` and `Intersect` are commutative but use the same binary form.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BoolOp {
    Union,
    Intersect,
    Subtract,
}

/// A node in a group's boolean expression tree. Leaves are shapes; internal
/// nodes combine two sub-trees with a [`BoolOp`]. For example
/// `A UNION (B SUBTRACT C)` is `Op { Union, Leaf(A), Op { Subtract, Leaf(B), Leaf(C) } }`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "node", rename_all = "snake_case")]
pub enum GroupNode {
    Leaf { shape: Shape },
    Op {
        op: BoolOp,
        left: Box<GroupNode>,
        right: Box<GroupNode>,
    },
}

impl GroupNode {
    /// Collect every leaf shape in this tree, in left-to-right order.
    pub fn shapes(&self) -> Vec<&Shape> {
        let mut out = Vec::new();
        self.collect_shapes(&mut out);
        out
    }

    fn collect_shapes<'a>(&'a self, out: &mut Vec<&'a Shape>) {
        match self {
            GroupNode::Leaf { shape } => out.push(shape),
            GroupNode::Op { left, right, .. } => {
                left.collect_shapes(out);
                right.collect_shapes(out);
            }
        }
    }
}

/// A collection of shapes combined via boolean operators. The group's own
/// `style` governs how the combined result is filled and outlined.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Group {
    pub id: GroupId,
    pub style: Style,
    pub root: GroupNode,
}

/// A rectangular play area: a grid of `width` x `height` squares, where each
/// square represents `grid_size` real-world units (e.g. `5.0` `"ft"`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Map {
    pub id: MapId,
    pub name: String,
    /// Width in grid units (squares).
    pub width: u32,
    /// Height in grid units (squares).
    pub height: u32,
    /// Real-world units represented by one square, e.g. `5.0`.
    pub grid_size: f64,
    /// Label for the real-world unit, e.g. `"ft"`.
    pub grid_unit: String,
    pub background_color: String,
    pub grid_color: String,
    /// Grouped shapes (each combined via boolean operators).
    pub groups: Vec<Group>,
    /// Standalone shapes that are not part of any group.
    pub shapes: Vec<Shape>,
}

/// Lightweight map description for the list view (no shapes/groups).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MapSummary {
    pub id: MapId,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub grid_size: f64,
    pub grid_unit: String,
}

// ---------------------------------------------------------------------------
// Maps: HTTP request/response DTOs
//
// The server assigns all ids, so create requests omit them. Update requests use
// `Option` fields so a client can patch a subset of properties.
// ---------------------------------------------------------------------------

/// Body for `POST /api/maps`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMapRequest {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub grid_size: f64,
    pub grid_unit: String,
    pub background_color: String,
    pub grid_color: String,
}

/// Body for `PUT /api/maps/{id}` (metadata only; contents are edited via the
/// shape/group endpoints).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateMapRequest {
    pub name: Option<String>,
    pub background_color: Option<String>,
    pub grid_color: Option<String>,
    pub grid_size: Option<f64>,
    pub grid_unit: Option<String>,
}

/// Body for `POST /api/maps/{id}/shapes`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateShapeRequest {
    pub geometry: Geometry,
    pub style: Style,
}

/// Body for `PUT /api/maps/{id}/shapes/{shape_id}`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateShapeRequest {
    pub geometry: Option<Geometry>,
    pub style: Option<Style>,
}

/// Body for `POST /api/maps/{id}/groups`. Accepts a full boolean tree so groups
/// can be created via the API (there is no group-builder UI yet).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateGroupRequest {
    pub style: Style,
    pub root: GroupNode,
}

/// Body for `PUT /api/maps/{id}/groups/{group_id}`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateGroupRequest {
    pub style: Option<Style>,
    pub root: Option<GroupNode>,
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
    /// Follow a map: the server will push that map's updates to this client.
    /// Following a new map replaces any previously followed map.
    FollowMap { map_id: MapId },
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
    /// Acknowledges a `FollowMap` request.
    FollowingMap { map_id: MapId },
    /// A followed map changed; carries the full updated map so the client
    /// never needs to refetch.
    MapUpdated { map: Map },
    /// Keepalive reply.
    Pong,
    /// An error to surface to the user.
    Error { message: String },
}
