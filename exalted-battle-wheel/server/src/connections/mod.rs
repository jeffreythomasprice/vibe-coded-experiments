//! The websocket-connections store: a bookkeeping record of every live connection, independent of
//! room membership (which lives on the room itself — see `crate::rooms`). Exists so a connection
//! that dies without a clean `Leave`/close handshake still ages out on its own, and so which access
//! code opened which connection is recorded somewhere. Shaped like `access_codes`/`rooms` — see
//! either module's own doc comment.

mod dynamo;
#[cfg(test)]
mod memory;

pub use dynamo::connect;
#[cfg(test)]
pub use memory::MemoryConnectionStore;

use aws_sdk_dynamodb::error::SdkError;
use aws_sdk_dynamodb::operation::delete_item::DeleteItemError;
use aws_sdk_dynamodb::operation::update_item::UpdateItemError;
use shared::protocol::ConnectionId;
use std::future::Future;

/// Matches `rooms::ROOM_TTL` — a connection and the room it's in age out on the same clock, so a
/// connection never outlives (or dies long before) the room membership it corresponds to.
pub const CONNECTION_TTL: time::Duration = time::Duration::minutes(30);

#[derive(Debug, thiserror::Error)]
pub enum ConnectionStoreError {
    #[error("dynamodb update_item failed")]
    UpdateItem(#[source] SdkError<UpdateItemError>),
    #[error("dynamodb delete_item failed")]
    DeleteItem(#[source] SdkError<DeleteItemError>),
}

/// Nothing in this store ever reads a `Connection` back into a Rust value — the room's own
/// `members` list (see `crate::rooms`) is the source of truth for who's connected and what they
/// may do; this table exists purely so a dead connection ages out even if it never sent a clean
/// close, and so which access code opened which connection is on record somewhere.
pub trait ConnectionStore: Clone + Send + Sync + 'static {
    /// Refreshes the TTL and records what this connection currently knows: the access code it
    /// last authenticated with (empty before its first message is validated), and the room (if
    /// any) it's currently in. Called once right after the socket is accepted (creating the row,
    /// since an `UpdateItem` with no condition creates one that doesn't exist yet) and again on
    /// every inbound message after that — even a rejected message is still "the client
    /// interacting."
    fn touch(
        &self,
        connection_id: &ConnectionId,
        access_key: &str,
        room_key: Option<&str>,
    ) -> impl Future<Output = Result<(), ConnectionStoreError>> + Send;
    /// Deletes the row immediately — never left for the TTL to reap, per this table's whole
    /// reason for existing.
    fn delete(&self, connection_id: &ConnectionId) -> impl Future<Output = Result<(), ConnectionStoreError>> + Send;
}
