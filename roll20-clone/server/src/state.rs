use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use shared::{ClientId, ServerMessage};
use tokio::sync::broadcast;
use turso::Database;

/// Shared application state.
///
/// This is a minimal "broadcast hub": every connected WebSocket subscribes to
/// `tx`, and anything published to it is fanned out to all clients. That's
/// enough to demonstrate real-time updates; rooms/persistence come later.
#[derive(Clone)]
pub struct AppState {
    tx: broadcast::Sender<ServerMessage>,
    next_client_id: Arc<AtomicU64>,
    db: Arc<Database>,
}

impl AppState {
    pub fn new(db: Arc<Database>) -> Self {
        let (tx, _rx) = broadcast::channel(256);
        Self {
            tx,
            next_client_id: Arc::new(AtomicU64::new(1)),
            db,
        }
    }

    /// The shared database. Call `.connect()` to obtain a connection for a query.
    #[allow(dead_code)]
    pub fn db(&self) -> &Database {
        &self.db
    }

    /// Subscribe a new connection to the broadcast stream.
    pub fn subscribe(&self) -> broadcast::Receiver<ServerMessage> {
        self.tx.subscribe()
    }

    /// Publish a message to every connected client. Ignores the "no receivers"
    /// error since clients may come and go.
    pub fn broadcast(&self, msg: ServerMessage) {
        let _ = self.tx.send(msg);
    }

    /// Allocate a fresh, unique client id.
    pub fn next_client_id(&self) -> ClientId {
        let n = self.next_client_id.fetch_add(1, Ordering::Relaxed);
        format!("client-{n}")
    }
}
