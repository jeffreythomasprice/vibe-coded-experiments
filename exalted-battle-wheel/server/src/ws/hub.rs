//! In-process fan-out from a room mutation to every connection currently in that room, plus the
//! single lock serializing every room mutation server-wide. Both are why `server/manifest.yaml`
//! pins `replicas: 1` — a member connected to a different pod would never receive a broadcast, and
//! two pods would each think they alone were serializing writes to the same DynamoDB item.

use axum::extract::ws::Message;
use shared::protocol::ConnectionId;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, MutexGuard};

pub type Outbox = mpsc::UnboundedSender<Message>;

#[derive(Clone, Default)]
pub struct Hub {
    inner: Arc<Inner>,
}

#[derive(Default)]
struct Inner {
    senders: Mutex<HashMap<ConnectionId, Outbox>>,
    /// Serializes every room mutation across the whole process. A lock per room (rather than one
    /// global lock) would let two *different* rooms' moves proceed concurrently, but this app's
    /// expected traffic — a handful of people advancing a tabletop combat tracker — makes that
    /// complexity not worth it: no bookkeeping for which room maps to which lock, and no question
    /// of when an unused one could ever be dropped.
    room_lock: tokio::sync::Mutex<()>,
}

impl Hub {
    pub fn register(&self, connection_id: ConnectionId, sender: Outbox) {
        self.inner.senders.lock().unwrap().insert(connection_id, sender);
    }

    pub fn unregister(&self, connection_id: &ConnectionId) {
        self.inner.senders.lock().unwrap().remove(connection_id);
    }

    /// Sends to one connection, if it's still registered. A send failure — its receiver already
    /// dropped, meaning that connection's own socket task is already tearing down — is silently
    /// ignored: whatever triggered that teardown handles the cleanup; there's nothing more to do
    /// about a message that's already too late to matter.
    pub fn send(&self, connection_id: &ConnectionId, message: Message) {
        if let Some(sender) = self.inner.senders.lock().unwrap().get(connection_id) {
            let _ = sender.send(message);
        }
    }

    /// Held for the whole of one `ws::handler::handle` call — see that module's doc comment on
    /// why every room mutation being serialized this way is what makes a `RoomStoreError::
    /// VersionConflict` unreachable in ordinary operation.
    pub async fn lock_rooms(&self) -> MutexGuard<'_, ()> {
        self.inner.room_lock.lock().await
    }
}
