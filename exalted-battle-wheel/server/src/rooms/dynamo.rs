use super::{ItemError, NewRoom, Room, RoomId, RoomMember, RoomPage, RoomQuery, RoomStore, RoomStoreError, MAX_ITEM_BYTES, ROOM_TTL};
use crate::config::Config;
use crate::dynamo_client::{self, format_timestamp};
use aws_sdk_dynamodb::operation::put_item::PutItemError;
use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_dynamodb::Client;
use shared::battle::BattleLog;
use shared::protocol::{ConnectionId, RoomName};
use shared::rooms::RoomSummary;
use shared::timestamp::Timestamp;
use std::collections::HashMap;
use std::sync::Arc;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use uuid::Uuid;

const ROOM_KEY: &str = "room_key";
const ROOM_ID: &str = "room_id";
const DISPLAY_NAME: &str = "display_name";
const VERSION: &str = "version";
const LOG: &str = "log";
const EVERYONE_WRITES: &str = "everyone_writes";
const MEMBERS: &str = "members";
const UPDATED_AT: &str = "updated_at";
const EXPIRES_AT: &str = "expires_at";

const MEMBER_CONNECTION_ID: &str = "connection_id";
const MEMBER_NAME: &str = "name";
const MEMBER_CAN_WRITE: &str = "can_write";
const MEMBER_IS_HOST: &str = "is_host";

#[derive(Clone)]
pub struct DynamoRoomStore {
    client: Client,
    table: Arc<str>,
}

pub async fn connect(config: &Config) -> DynamoRoomStore {
    DynamoRoomStore { client: dynamo_client::client(config).await, table: Arc::from(config.rooms_table.as_str()) }
}

/// Rough but conservative: DynamoDB's per-item ceiling is on the whole item, but the log
/// dominates every other field by orders of magnitude, so approximating with just the fields that
/// actually scale is enough to catch an oversized battle well before the real `PutItem` would
/// reject it with an opaque `ValidationException`.
fn approximate_size(log_json: &str, room_key: &str, display_name: &str, members: &[RoomMember]) -> usize {
    log_json.len()
        + room_key.len()
        + display_name.len()
        // A room id is a fixed-width UUID string, small enough not to need its own parameter --
        // folded into the constant padding below instead.
        + members.iter().map(|member| member.name.len() + 64).sum::<usize>()
        + 256
}

fn check_size(log_json: &str, room_key: &str, display_name: &str, members: &[RoomMember]) -> Result<(), RoomStoreError> {
    let size = approximate_size(log_json, room_key, display_name, members);
    if size > MAX_ITEM_BYTES {
        return Err(RoomStoreError::TooLarge { size });
    }
    Ok(())
}

fn epoch_seconds(at: OffsetDateTime) -> String {
    at.unix_timestamp().to_string()
}

fn member_to_item(member: &RoomMember) -> AttributeValue {
    AttributeValue::M(HashMap::from([
        (MEMBER_CONNECTION_ID.to_string(), AttributeValue::S(member.connection_id.0.clone())),
        (MEMBER_NAME.to_string(), AttributeValue::S(member.name.clone())),
        (MEMBER_CAN_WRITE.to_string(), AttributeValue::Bool(member.can_write)),
        (MEMBER_IS_HOST.to_string(), AttributeValue::Bool(member.is_host)),
    ]))
}

fn item_to_member(item: &HashMap<String, AttributeValue>) -> Result<RoomMember, ItemError> {
    Ok(RoomMember {
        connection_id: ConnectionId(string_attr(item, MEMBER_CONNECTION_ID)?),
        name: string_attr(item, MEMBER_NAME)?,
        can_write: bool_attr(item, MEMBER_CAN_WRITE)?,
        is_host: bool_attr(item, MEMBER_IS_HOST)?,
    })
}

fn room_to_item(room: &Room, log_json: &str) -> HashMap<String, AttributeValue> {
    HashMap::from([
        (ROOM_KEY.to_string(), AttributeValue::S(room.room_key.clone())),
        (ROOM_ID.to_string(), AttributeValue::S(room.id.0.clone())),
        (DISPLAY_NAME.to_string(), AttributeValue::S(room.display_name.clone())),
        (VERSION.to_string(), AttributeValue::N(room.version.to_string())),
        (LOG.to_string(), AttributeValue::S(log_json.to_string())),
        (EVERYONE_WRITES.to_string(), AttributeValue::Bool(room.everyone_writes)),
        (MEMBERS.to_string(), AttributeValue::L(room.members.iter().map(member_to_item).collect())),
        (UPDATED_AT.to_string(), AttributeValue::S(format_timestamp(room.updated_at))),
        (EXPIRES_AT.to_string(), AttributeValue::N(epoch_seconds(room.expires_at))),
    ])
}

fn item_to_room(item: &HashMap<String, AttributeValue>) -> Result<Room, ItemError> {
    let log_json = string_attr(item, LOG)?;
    let log: BattleLog = serde_json::from_str(&log_json).map_err(|source| ItemError::Json { name: LOG, source })?;
    let members = match item.get(MEMBERS) {
        Some(AttributeValue::L(entries)) => entries
            .iter()
            .map(|entry| match entry {
                AttributeValue::M(map) => item_to_member(map),
                _ => Err(ItemError::WrongType { name: MEMBERS, expected: "L of M" }),
            })
            .collect::<Result<Vec<_>, _>>()?,
        Some(_) => return Err(ItemError::WrongType { name: MEMBERS, expected: "L" }),
        None => return Err(ItemError::Missing(MEMBERS)),
    };

    Ok(Room {
        id: RoomId(string_attr(item, ROOM_ID)?),
        room_key: string_attr(item, ROOM_KEY)?,
        display_name: string_attr(item, DISPLAY_NAME)?,
        version: u64_attr(item, VERSION)?,
        log,
        everyone_writes: bool_attr(item, EVERYONE_WRITES)?,
        members,
        updated_at: timestamp_attr(item, UPDATED_AT)?,
        expires_at: epoch_attr(item, EXPIRES_AT)?,
    })
}

fn string_attr(item: &HashMap<String, AttributeValue>, name: &'static str) -> Result<String, ItemError> {
    match item.get(name) {
        Some(AttributeValue::S(value)) => Ok(value.clone()),
        Some(_) => Err(ItemError::WrongType { name, expected: "S" }),
        None => Err(ItemError::Missing(name)),
    }
}

fn bool_attr(item: &HashMap<String, AttributeValue>, name: &'static str) -> Result<bool, ItemError> {
    match item.get(name) {
        Some(AttributeValue::Bool(value)) => Ok(*value),
        Some(_) => Err(ItemError::WrongType { name, expected: "BOOL" }),
        None => Err(ItemError::Missing(name)),
    }
}

fn u64_attr(item: &HashMap<String, AttributeValue>, name: &'static str) -> Result<u64, ItemError> {
    let value = match item.get(name) {
        Some(AttributeValue::N(value)) => value.clone(),
        Some(_) => return Err(ItemError::WrongType { name, expected: "N" }),
        None => return Err(ItemError::Missing(name)),
    };
    value.parse().map_err(|source| ItemError::Number { name, value, source })
}

fn timestamp_attr(item: &HashMap<String, AttributeValue>, name: &'static str) -> Result<OffsetDateTime, ItemError> {
    let value = string_attr(item, name)?;
    OffsetDateTime::parse(&value, &Rfc3339).map_err(|source| ItemError::Timestamp { name, value, source })
}

fn epoch_attr(item: &HashMap<String, AttributeValue>, name: &'static str) -> Result<OffsetDateTime, ItemError> {
    let value = match item.get(name) {
        Some(AttributeValue::N(value)) => value.clone(),
        Some(_) => return Err(ItemError::WrongType { name, expected: "N" }),
        None => return Err(ItemError::Missing(name)),
    };
    let seconds: i64 = value.parse().map_err(|source| ItemError::Number { name, value, source })?;
    OffsetDateTime::from_unix_timestamp(seconds).map_err(|source| ItemError::Epoch { name, value: seconds, source })
}

impl RoomStore for DynamoRoomStore {
    async fn get(&self, room_key: &str) -> Result<Option<Room>, RoomStoreError> {
        let output = self
            .client
            .get_item()
            .table_name(&*self.table)
            .key(ROOM_KEY, AttributeValue::S(room_key.to_string()))
            .consistent_read(true)
            .send()
            .await
            .map_err(RoomStoreError::GetItem)?;

        let Some(item) = output.item() else { return Ok(None) };
        let room = item_to_room(item)?;
        if room.expires_at <= OffsetDateTime::now_utc() {
            return Ok(None);
        }
        Ok(Some(room))
    }

    async fn list(&self, query: &RoomQuery) -> Result<RoomPage, RoomStoreError> {
        // Projected, not a plain `scan()`: a `RoomSummary` needs a handful of small fields, but a
        // room's `log` can be up to `MAX_ITEM_BYTES` -- fetching (and JSON-decoding, via
        // `item_to_room`) every room's whole battle just to report its member count would scale
        // this endpoint's cost with total battle data across every room, not with the size of its
        // own response.
        //
        // The search and the page boundary are both applied here in Rust rather than as a
        // DynamoDB `FilterExpression`/`Limit`: a `FilterExpression` still pays for scanning every
        // *unfiltered* item (it just skips returning the ones that don't match), so a `Limit`
        // paired with it would cap items scanned, not items matched, and could hand back an
        // under-full page while rooms matching the search still existed further into the table.
        // Scanning every raw page to completion and stopping once `query.limit` *matches* have
        // been found is what actually makes `next` mean "there may be more."
        let needle = query.search.trim().to_lowercase();
        // A caller is expected to have already clamped this (`server/src/routes.rs`'s
        // `room_query`) -- floored rather than trusted outright so a stray zero can't turn every
        // page into an infinite scan that never finds enough matches to stop on.
        let limit = query.limit.max(1);
        let now = OffsetDateTime::now_utc();
        let mut rooms = Vec::new();
        let mut next = None;
        let mut start_key =
            query.after.as_ref().map(|after| HashMap::from([(ROOM_KEY.to_string(), AttributeValue::S(after.clone()))]));

        'paging: loop {
            let output = self
                .client
                .scan()
                .table_name(&*self.table)
                .projection_expression("#room_key, #display_name, #members, #updated_at, #expires_at")
                .expression_attribute_names("#room_key", ROOM_KEY)
                .expression_attribute_names("#display_name", DISPLAY_NAME)
                .expression_attribute_names("#members", MEMBERS)
                .expression_attribute_names("#updated_at", UPDATED_AT)
                .expression_attribute_names("#expires_at", EXPIRES_AT)
                .set_exclusive_start_key(start_key.take())
                .send()
                .await
                .map_err(RoomStoreError::Scan)?;

            for item in output.items() {
                if epoch_attr(item, EXPIRES_AT)? <= now {
                    continue;
                }
                let room_key = string_attr(item, ROOM_KEY)?;
                if !needle.is_empty() && !room_key.contains(&needle) {
                    continue;
                }
                let member_count = match item.get(MEMBERS) {
                    Some(AttributeValue::L(entries)) => u32::try_from(entries.len()).unwrap_or(u32::MAX),
                    Some(_) => return Err(ItemError::WrongType { name: MEMBERS, expected: "L" }.into()),
                    None => return Err(ItemError::Missing(MEMBERS).into()),
                };
                let display_name = string_attr(item, DISPLAY_NAME)?;
                rooms.push(RoomSummary {
                    display_name: RoomName::try_from(display_name.clone()).map_err(|error| ItemError::Invalid {
                        name: DISPLAY_NAME,
                        value: display_name,
                        reason: error.to_string(),
                    })?,
                    member_count,
                    updated_at: Timestamp(timestamp_attr(item, UPDATED_AT)?),
                });
                if rooms.len() == limit {
                    next = Some(room_key);
                    break 'paging;
                }
            }

            match output.last_evaluated_key() {
                Some(key) => start_key = Some(key.clone()),
                None => break,
            }
        }

        Ok(RoomPage { rooms, next })
    }

    async fn delete(&self, room_key: &str) -> Result<(), RoomStoreError> {
        self.client
            .delete_item()
            .table_name(&*self.table)
            .key(ROOM_KEY, AttributeValue::S(room_key.to_string()))
            .send()
            .await
            .map_err(RoomStoreError::DeleteItem)?;
        Ok(())
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
        let log_json = serde_json::to_string(&room.log).expect("BattleLog always encodes");
        check_size(&log_json, &room.room_key, &room.display_name, &room.members)?;

        let result = self
            .client
            .put_item()
            .table_name(&*self.table)
            .set_item(Some(room_to_item(&room, &log_json)))
            .condition_expression("attribute_not_exists(#room_key) OR #expires_at <= :now")
            .expression_attribute_names("#room_key", ROOM_KEY)
            .expression_attribute_names("#expires_at", EXPIRES_AT)
            .expression_attribute_values(":now", AttributeValue::N(epoch_seconds(now)))
            .send()
            .await;

        match result {
            Ok(_) => Ok(room),
            Err(error) if error.as_service_error().is_some_and(PutItemError::is_conditional_check_failed_exception) => {
                Err(RoomStoreError::AlreadyExists)
            }
            Err(error) => Err(RoomStoreError::PutItem(error)),
        }
    }

    async fn save(&self, mut room: Room) -> Result<Room, RoomStoreError> {
        let log_json = serde_json::to_string(&room.log).expect("BattleLog always encodes");
        check_size(&log_json, &room.room_key, &room.display_name, &room.members)?;

        let expected_version = room.version;
        let now = OffsetDateTime::now_utc();
        room.version = expected_version + 1;
        room.updated_at = now;
        room.expires_at = now + ROOM_TTL;

        let result = self
            .client
            .put_item()
            .table_name(&*self.table)
            .set_item(Some(room_to_item(&room, &log_json)))
            .condition_expression("#version = :expected_version")
            .expression_attribute_names("#version", VERSION)
            .expression_attribute_values(":expected_version", AttributeValue::N(expected_version.to_string()))
            .send()
            .await;

        match result {
            Ok(_) => Ok(room),
            Err(error) if error.as_service_error().is_some_and(PutItemError::is_conditional_check_failed_exception) => {
                Err(RoomStoreError::VersionConflict)
            }
            Err(error) => Err(RoomStoreError::PutItem(error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Room {
        Room {
            id: RoomId("11111111-1111-4111-8111-111111111111".to_string()),
            room_key: "test-room".to_string(),
            display_name: "Test Room".to_string(),
            version: 3,
            log: BattleLog::new(),
            everyone_writes: true,
            members: vec![RoomMember { connection_id: ConnectionId("host-conn".to_string()), name: "Host".to_string(), can_write: true, is_host: true }],
            updated_at: OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap(),
            expires_at: OffsetDateTime::from_unix_timestamp(1_700_001_800).unwrap(),
        }
    }

    #[test]
    fn item_round_trips() {
        let room = sample();
        let log_json = serde_json::to_string(&room.log).unwrap();
        assert_eq!(item_to_room(&room_to_item(&room, &log_json)).unwrap(), room);
    }

    #[test]
    fn item_round_trips_with_a_non_host_member() {
        let mut room = sample();
        room.members.push(RoomMember { connection_id: ConnectionId("other-conn".to_string()), name: "Other".to_string(), can_write: false, is_host: false });
        let log_json = serde_json::to_string(&room.log).unwrap();
        assert_eq!(item_to_room(&room_to_item(&room, &log_json)).unwrap(), room);
    }

    #[test]
    fn missing_attribute_is_an_error() {
        let room = sample();
        let log_json = serde_json::to_string(&room.log).unwrap();
        let mut item = room_to_item(&room, &log_json);
        item.remove(EVERYONE_WRITES);
        assert!(matches!(item_to_room(&item), Err(ItemError::Missing(EVERYONE_WRITES))));
    }

    #[test]
    fn approximate_size_grows_with_the_log() {
        let small = approximate_size("{}", "room", "Room", &[]);
        let large = approximate_size(&"x".repeat(1000), "room", "Room", &[]);
        assert!(large > small);
    }
}
