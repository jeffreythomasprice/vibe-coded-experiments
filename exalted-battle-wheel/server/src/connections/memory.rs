//! An in-memory fake for `ConnectionStore` — nothing checks conditional semantics here (unlike
//! `rooms`/`access_codes`, there's no conflict to reproduce), so this just needs to be a working
//! stand-in wherever a test needs some `ConnectionStore` and doesn't care which.

use super::{ConnectionStore, ConnectionStoreError};
use shared::protocol::ConnectionId;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct MemoryConnectionStore {
    live: Arc<Mutex<HashSet<String>>>,
}

impl ConnectionStore for MemoryConnectionStore {
    async fn touch(&self, connection_id: &ConnectionId, _access_key: &str, _room_key: Option<&str>) -> Result<(), ConnectionStoreError> {
        self.live.lock().unwrap().insert(connection_id.0.clone());
        Ok(())
    }

    async fn delete(&self, connection_id: &ConnectionId) -> Result<(), ConnectionStoreError> {
        self.live.lock().unwrap().remove(&connection_id.0);
        Ok(())
    }
}
