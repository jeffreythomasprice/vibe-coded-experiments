use super::{ConnectionStore, ConnectionStoreError, CONNECTION_TTL};
use crate::config::Config;
use crate::dynamo_client::{self, format_timestamp};
use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_dynamodb::Client;
use shared::protocol::ConnectionId;
use std::sync::Arc;
use time::OffsetDateTime;

const CONNECTION_ID: &str = "connection_id";
const ACCESS_KEY: &str = "access_key";
const ROOM_KEY: &str = "room_key";
const CONNECTED_AT: &str = "connected_at";
const EXPIRES_AT: &str = "expires_at";

#[derive(Clone)]
pub struct DynamoConnectionStore {
    client: Client,
    table: Arc<str>,
}

pub async fn connect(config: &Config) -> DynamoConnectionStore {
    DynamoConnectionStore { client: dynamo_client::client(config).await, table: Arc::from(config.connections_table.as_str()) }
}

impl ConnectionStore for DynamoConnectionStore {
    async fn touch(&self, connection_id: &ConnectionId, access_key: &str, room_key: Option<&str>) -> Result<(), ConnectionStoreError> {
        let now = OffsetDateTime::now_utc();
        // No condition expression, so this creates the row on the very first call (right after
        // accept, before any message has even arrived) exactly as readily as it updates one that
        // already exists — no separate "create" step needed. `if_not_exists` keeps `connected_at`
        // pinned to that first call rather than sliding forward on every later touch.
        let mut request = self
            .client
            .update_item()
            .table_name(&*self.table)
            .key(CONNECTION_ID, AttributeValue::S(connection_id.0.clone()))
            .update_expression(if room_key.is_some() {
                "SET #access_key = :access_key, #connected_at = if_not_exists(#connected_at, :now), #room_key = :room_key, #expires_at = :expires_at"
            } else {
                "SET #access_key = :access_key, #connected_at = if_not_exists(#connected_at, :now), #expires_at = :expires_at REMOVE #room_key"
            })
            .expression_attribute_names("#access_key", ACCESS_KEY)
            .expression_attribute_names("#connected_at", CONNECTED_AT)
            .expression_attribute_names("#room_key", ROOM_KEY)
            .expression_attribute_names("#expires_at", EXPIRES_AT)
            .expression_attribute_values(":access_key", AttributeValue::S(access_key.to_string()))
            .expression_attribute_values(":now", AttributeValue::S(format_timestamp(now)))
            .expression_attribute_values(":expires_at", AttributeValue::N((now + CONNECTION_TTL).unix_timestamp().to_string()));
        if let Some(room_key) = room_key {
            request = request.expression_attribute_values(":room_key", AttributeValue::S(room_key.to_string()));
        }
        request.send().await.map_err(ConnectionStoreError::UpdateItem)?;
        Ok(())
    }

    async fn delete(&self, connection_id: &ConnectionId) -> Result<(), ConnectionStoreError> {
        self.client
            .delete_item()
            .table_name(&*self.table)
            .key(CONNECTION_ID, AttributeValue::S(connection_id.0.clone()))
            .send()
            .await
            .map_err(ConnectionStoreError::DeleteItem)?;
        Ok(())
    }
}
