//! An in-memory fake reproducing `DynamoRoomStore`'s conditional-write semantics (`AlreadyExists`
//! on creating over a live room, `VersionConflict` on a stale `save`, `NotFound`/expiry treated the
//! same as a real read), so websocket-handler tests can exercise real behavior with no Docker or
//! DynamoDB.

use super::{NewRoom, Room, RoomId, RoomMember, RoomStore, RoomStoreError, MAX_ITEM_BYTES, ROOM_TTL};
use shared::rooms::RoomSummary;
use shared::timestamp::Timestamp;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone, Default)]
pub struct MemoryRoomStore {
    rooms: Arc<Mutex<HashMap<String, Room>>>,
}

fn approximate_size(room: &Room) -> usize {
    serde_json::to_string(&room.log).map(|json| json.len()).unwrap_or(0)
        + room.room_key.len()
        + room.display_name.len()
        + room.members.iter().map(|member| member.name.len() + 64).sum::<usize>()
        + 256
}

impl RoomStore for MemoryRoomStore {
    async fn get(&self, room_key: &str) -> Result<Option<Room>, RoomStoreError> {
        let rooms = self.rooms.lock().unwrap();
        Ok(rooms.get(room_key).filter(|room| room.expires_at > OffsetDateTime::now_utc()).cloned())
    }

    async fn list(&self) -> Result<Vec<RoomSummary>, RoomStoreError> {
        let rooms = self.rooms.lock().unwrap();
        let now = OffsetDateTime::now_utc();
        Ok(rooms
            .values()
            .filter(|room| room.expires_at > now)
            .map(|room| RoomSummary {
                display_name: room.display_name.clone().try_into().expect("valid by construction: see ws/handler.rs"),
                member_count: u32::try_from(room.members.len()).unwrap_or(u32::MAX),
                updated_at: Timestamp(room.updated_at),
            })
            .collect())
    }

    async fn create(&self, new_room: NewRoom) -> Result<Room, RoomStoreError> {
        let now = OffsetDateTime::now_utc();
        let room = Room {
            id: RoomId(Uuid::new_v4().to_string()),
            room_key: new_room.room_key,
            display_name: new_room.display_name,
            version: 1,
            log: new_room.log,
            everyone_writes: new_room.everyone_writes,
            members: vec![RoomMember { connection_id: new_room.host, name: new_room.host_name, can_write: true, is_host: true }],
            updated_at: now,
            expires_at: now + ROOM_TTL,
        };
        if approximate_size(&room) > MAX_ITEM_BYTES {
            return Err(RoomStoreError::TooLarge { size: approximate_size(&room) });
        }

        let mut rooms = self.rooms.lock().unwrap();
        if rooms.get(&room.room_key).is_some_and(|existing| existing.expires_at > now) {
            return Err(RoomStoreError::AlreadyExists);
        }
        rooms.insert(room.room_key.clone(), room.clone());
        Ok(room)
    }

    async fn save(&self, mut room: Room) -> Result<Room, RoomStoreError> {
        if approximate_size(&room) > MAX_ITEM_BYTES {
            return Err(RoomStoreError::TooLarge { size: approximate_size(&room) });
        }

        let mut rooms = self.rooms.lock().unwrap();
        let expected_version = room.version;
        let stored_version = rooms.get(&room.room_key).map(|existing| existing.version);
        if stored_version != Some(expected_version) {
            return Err(RoomStoreError::VersionConflict);
        }

        let now = OffsetDateTime::now_utc();
        room.version = expected_version + 1;
        room.updated_at = now;
        room.expires_at = now + ROOM_TTL;
        rooms.insert(room.room_key.clone(), room.clone());
        Ok(room)
    }
}

impl MemoryRoomStore {
    /// Seeds a room directly, bypassing `create`, so tests can set up a room in whatever exact
    /// state (no member currently holding host, `everyone_writes` off, a particular battle) they
    /// need to check.
    pub fn seed(&self, room: Room) {
        self.rooms.lock().unwrap().insert(room.room_key.clone(), room);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::battle::BattleLog;
    use shared::protocol::ConnectionId;

    fn new_room(key: &str) -> NewRoom {
        NewRoom {
            room_key: key.to_string(),
            display_name: key.to_string(),
            everyone_writes: true,
            host: ConnectionId("host".to_string()),
            host_name: "Host".to_string(),
            log: BattleLog::new(),
        }
    }

    #[tokio::test]
    async fn creating_over_a_live_room_is_already_exists() {
        let store = MemoryRoomStore::default();
        store.create(new_room("room")).await.unwrap();
        assert!(matches!(store.create(new_room("room")).await, Err(RoomStoreError::AlreadyExists)));
    }

    #[tokio::test]
    async fn creating_over_an_expired_room_succeeds() {
        let store = MemoryRoomStore::default();
        let mut expired = store.create(new_room("room")).await.unwrap();
        expired.expires_at = OffsetDateTime::now_utc() - time::Duration::seconds(1);
        store.seed(expired);

        assert!(store.create(new_room("room")).await.is_ok());
    }

    #[tokio::test]
    async fn get_treats_an_expired_room_as_missing() {
        let store = MemoryRoomStore::default();
        let mut expired = store.create(new_room("room")).await.unwrap();
        expired.expires_at = OffsetDateTime::now_utc() - time::Duration::seconds(1);
        store.seed(expired);

        assert_eq!(store.get("room").await.unwrap(), None);
    }

    #[tokio::test]
    async fn save_bumps_the_version_and_refreshes_the_ttl() {
        let store = MemoryRoomStore::default();
        let room = store.create(new_room("room")).await.unwrap();
        assert_eq!(room.version, 1);

        let saved = store.save(room).await.unwrap();
        assert_eq!(saved.version, 2);
    }

    #[tokio::test]
    async fn saving_a_stale_version_is_a_conflict() {
        let store = MemoryRoomStore::default();
        let room = store.create(new_room("room")).await.unwrap();
        store.save(room.clone()).await.unwrap();

        // `room` still carries the version it was created at -- now one behind what's stored.
        assert!(matches!(store.save(room).await, Err(RoomStoreError::VersionConflict)));
    }
}
