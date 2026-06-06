//! The engine's registry of connected clients.
//!
//! Shared between the accept loop, every connection task, and the reaper, behind
//! a plain [`std::sync::Mutex`]. Every critical section here is short and is
//! *never* held across an `.await` (see `server.rs`), so a blocking mutex is
//! both correct and cheaper than an async one.
//!
//! Each client carries two timestamps for its last activity: a [`SystemTime`]
//! for reporting on the wire, and a monotonic [`Instant`] that the reaper uses
//! for idle math (immune to wall-clock jumps / NTP steps).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tokio::sync::oneshot;

use shared::protocol::ClientInfo;

/// Per-client bookkeeping held by the [`Registry`].
pub struct ClientEntry {
    /// Name the client gave in its `Hello` (empty until then).
    name: String,
    /// When the client connected (wall clock, for reporting).
    connected_at: SystemTime,
    /// When the client last sent a message (wall clock, for reporting).
    last_seen_wall: SystemTime,
    /// When the client last sent a message (monotonic, for the reaper).
    last_seen: Instant,
    /// Fires to tell the connection task to drop this client. `take`n when the
    /// reaper decides to kick, so a client is signalled at most once.
    kick: Option<oneshot::Sender<()>>,
}

/// The set of connected clients plus the next id to hand out.
pub struct Registry {
    clients: HashMap<u64, ClientEntry>,
    next_id: u64,
}

/// The registry, shared across the server's tasks.
pub type SharedRegistry = Arc<Mutex<Registry>>;

impl Registry {
    /// Create a new, empty registry wrapped for sharing.
    pub fn new_shared() -> SharedRegistry {
        Arc::new(Mutex::new(Registry {
            clients: HashMap::new(),
            next_id: 1,
        }))
    }

    /// Register a freshly accepted connection, returning its assigned id. The
    /// `kick` sender lets the reaper later tell this client's task to hang up.
    pub fn insert(&mut self, kick: oneshot::Sender<()>) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let now_wall = SystemTime::now();
        self.clients.insert(
            id,
            ClientEntry {
                name: String::new(),
                connected_at: now_wall,
                last_seen_wall: now_wall,
                last_seen: Instant::now(),
                kick: Some(kick),
            },
        );
        id
    }

    /// Remove a client, on disconnect or after a kick.
    pub fn remove(&mut self, id: u64) {
        self.clients.remove(&id);
    }

    /// Record activity from a client: refresh its idle timers, and set its name
    /// when `name` is given (i.e. on the `Hello`).
    pub fn touch(&mut self, id: u64, name: Option<&str>) {
        if let Some(entry) = self.clients.get_mut(&id) {
            entry.last_seen = Instant::now();
            entry.last_seen_wall = SystemTime::now();
            if let Some(name) = name {
                entry.name = name.to_owned();
            }
        }
    }

    /// Build a point-in-time snapshot of every client for a `Stats` reply,
    /// sorted by id for stable output.
    pub fn snapshot(&self) -> Vec<ClientInfo> {
        let now = Instant::now();
        let mut clients: Vec<ClientInfo> = self
            .clients
            .iter()
            .map(|(&id, e)| ClientInfo {
                id,
                name: e.name.clone(),
                connected_at_epoch_ms: epoch_ms(e.connected_at),
                last_ping_epoch_ms: epoch_ms(e.last_seen_wall),
                idle_ms: now.duration_since(e.last_seen).as_millis() as u64,
            })
            .collect();
        clients.sort_by_key(|c| c.id);
        clients
    }

    /// Signal every client idle longer than `max_idle` to disconnect. The
    /// client's own connection task performs the actual [`Registry::remove`],
    /// so the kick sender is `take`n here to ensure it's only signalled once.
    pub fn reap(&mut self, max_idle: Duration) {
        let now = Instant::now();
        for entry in self.clients.values_mut() {
            if now.duration_since(entry.last_seen) > max_idle {
                if let Some(tx) = entry.kick.take() {
                    let _ = tx.send(());
                }
            }
        }
    }
}

/// Convert a [`SystemTime`] to Unix epoch milliseconds (0 if before the epoch).
fn epoch_ms(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
