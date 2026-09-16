//! The room store: what a room is, the storage-agnostic interface the websocket handler uses, and
//! the errors that can come out of it. `dynamo` is the real implementation; `memory` is a
//! test-only fake with the same conditional-write semantics. Shaped like `access_codes` — see that
//! module's own doc comment.

mod dynamo;
#[cfg(test)]
mod memory;

pub use dynamo::connect;
#[cfg(test)]
pub use memory::MemoryRoomStore;

use crate::dynamo_client::ItemError;
use aws_sdk_dynamodb::error::SdkError;
use aws_sdk_dynamodb::operation::put_item::PutItemError;
use aws_sdk_dynamodb::operation::scan::ScanError;
use shared::battle::BattleLog;
use shared::protocol::ConnectionId;
use shared::rooms::RoomSummary;
use std::future::Future;
use time::OffsetDateTime;

/// How long an idle room is kept before it's eligible for reaping — bumped on every change to the
/// room, membership included. See `CLAUDE.md`/the room-store doc for why a room outlives its last
/// member rather than being deleted the moment it empties out.
pub const ROOM_TTL: time::Duration = time::Duration::minutes(30);

/// DynamoDB's hard per-item cap. Checked before every write so an oversized battle is a clear
/// `RoomStoreError::TooLarge` rather than an opaque `ValidationException` from the service.
pub const MAX_ITEM_BYTES: usize = 400 * 1024;

/// A room's own identity, independent of its (reusable, case-folded) name — what a session token
/// is actually issued and checked against, so a token for a room that expired and whose name was
/// later reused by a different room is never mistaken for one. See `server::sessions`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomId(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomMember {
    pub connection_id: ConnectionId,
    pub name: String,
    pub can_write: bool,
    /// This connection either created the room, or rejoined it presenting a session token that
    /// said it had (see `shared::protocol::ClientMessage::Join`'s `session` field). Never demoted,
    /// never kicked. Lives on the member rather than the room so more than one connection can hold
    /// it at once — a rejoin claiming host status doesn't have to know or care whether the
    /// original host's connection is still around.
    pub is_host: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Room {
    pub id: RoomId,
    pub room_key: String,
    pub display_name: String,
    pub version: u64,
    pub log: BattleLog,
    pub everyone_writes: bool,
    pub members: Vec<RoomMember>,
    pub updated_at: OffsetDateTime,
    pub expires_at: OffsetDateTime,
}

impl Room {
    pub fn member(&self, connection_id: &ConnectionId) -> Option<&RoomMember> {
        self.members.iter().find(|member| &member.connection_id == connection_id)
    }

    pub fn is_host(&self, connection_id: &ConnectionId) -> bool {
        self.member(connection_id).is_some_and(|member| member.is_host)
    }
}

/// What `RoomStore::create` needs, bundled so the trait method doesn't grow an unreadable
/// parameter list. The new room starts with exactly one member: the host, always writable
/// regardless of `everyone_writes` (that flag only governs a *future* joiner — see
/// `shared::protocol::ClientMessage::Create`'s doc comment).
pub struct NewRoom {
    pub room_key: String,
    pub display_name: String,
    pub everyone_writes: bool,
    pub host: ConnectionId,
    pub host_name: String,
    pub log: BattleLog,
}

#[derive(Debug, thiserror::Error)]
pub enum RoomStoreError {
    #[error("a room with that name already exists")]
    AlreadyExists,
    /// Only possible if two writers raced past the in-process room lock somehow — see
    /// `server/src/ws/hub.rs`'s doc comment on why that lock should make this unreachable in
    /// practice. Surfaced rather than silently retried: a retry could rebase an already-applied
    /// game move onto a state it was never actually checked against.
    #[error("the room changed underneath this write")]
    VersionConflict,
    #[error("that battle is too large to store ({size} bytes, over the {MAX_ITEM_BYTES}-byte limit)")]
    TooLarge { size: usize },
    #[error(transparent)]
    Item(#[from] ItemError),
    #[error("dynamodb get_item failed")]
    GetItem(#[source] SdkError<aws_sdk_dynamodb::operation::get_item::GetItemError>),
    #[error("dynamodb put_item failed")]
    PutItem(#[source] SdkError<PutItemError>),
    #[error("dynamodb scan failed")]
    Scan(#[source] SdkError<ScanError>),
}

pub trait RoomStore: Clone + Send + Sync + 'static {
    /// `None` for both a genuinely missing room and one whose `expires_at` has already passed —
    /// real DynamoDB reaps an expired item up to 48 hours late, and dynamodb-local never reaps at
    /// all, so every read has to treat "expired" as "not there" itself rather than trusting the
    /// table to have already deleted it.
    fn get(&self, room_key: &str) -> impl Future<Output = Result<Option<Room>, RoomStoreError>> + Send;
    /// Every non-expired room, for `GET /rooms`.
    fn list(&self) -> impl Future<Output = Result<Vec<RoomSummary>, RoomStoreError>> + Send;
    /// `RoomStoreError::AlreadyExists` if a non-expired room already holds this name.
    fn create(&self, room: NewRoom) -> impl Future<Output = Result<Room, RoomStoreError>> + Send;
    /// Saves every field of `room` back, conditioned on the stored version still matching
    /// `room.version` (the version it was read at) — the caller `get`s a `Room`, mutates a working
    /// copy, and passes that copy here unchanged apart from whatever it meant to change. Bumps the
    /// version and refreshes `expires_at`/`updated_at`, returning the room as actually stored.
    fn save(&self, room: Room) -> impl Future<Output = Result<Room, RoomStoreError>> + Send;
}
