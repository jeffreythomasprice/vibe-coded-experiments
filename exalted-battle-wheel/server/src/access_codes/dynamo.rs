use super::{generate_key, AccessCode, AccessCodeStore, ItemError, StoreError};
use crate::config::Config;
use crate::dynamo_client::{self, format_timestamp};
use aws_sdk_dynamodb::operation::delete_item::DeleteItemError;
use aws_sdk_dynamodb::operation::put_item::PutItemError;
use aws_sdk_dynamodb::operation::update_item::UpdateItemError;
use aws_sdk_dynamodb::types::{AttributeValue, ReturnValue};
use aws_sdk_dynamodb::Client;
use std::collections::HashMap;
use std::sync::Arc;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

const ACCESS_KEY: &str = "access_key";
const IS_ADMIN: &str = "is_admin";
const CREATED_AT: &str = "created_at";

#[derive(Clone)]
pub struct DynamoAccessCodeStore {
    client: Client,
    table: Arc<str>,
}

pub async fn connect(config: &Config) -> DynamoAccessCodeStore {
    DynamoAccessCodeStore { client: dynamo_client::client(config).await, table: Arc::from(config.access_codes_table.as_str()) }
}

impl AccessCodeStore for DynamoAccessCodeStore {
    async fn get(&self, access_key: &str) -> Result<Option<AccessCode>, StoreError> {
        let output = self
            .client
            .get_item()
            .table_name(&*self.table)
            .key(ACCESS_KEY, AttributeValue::S(access_key.to_string()))
            // Every authenticated request lands here -- an eventually consistent read would 401 a
            // code for a second right after an admin creates it.
            .consistent_read(true)
            .send()
            .await
            .map_err(StoreError::GetItem)?;

        output.item().map(item_to_access_code).transpose().map_err(StoreError::from)
    }

    async fn list(&self) -> Result<Vec<AccessCode>, StoreError> {
        let mut pages = self.client.scan().table_name(&*self.table).into_paginator().items().send();
        let mut codes = Vec::new();
        while let Some(item) = pages.next().await {
            codes.push(item_to_access_code(&item.map_err(StoreError::Scan)?)?);
        }
        Ok(codes)
    }

    async fn create(&self, access_key: Option<&str>, is_admin: bool) -> Result<AccessCode, StoreError> {
        let access_key = match access_key {
            Some(access_key) => access_key.to_string(),
            None => generate_key()?,
        };
        let code = AccessCode { access_key, is_admin, created_at: OffsetDateTime::now_utc() };

        let result = self
            .client
            .put_item()
            .table_name(&*self.table)
            .set_item(Some(access_code_to_item(&code)))
            .condition_expression("attribute_not_exists(#access_key)")
            .expression_attribute_names("#access_key", ACCESS_KEY)
            .send()
            .await;

        match result {
            Ok(_) => Ok(code),
            Err(error) if error.as_service_error().is_some_and(PutItemError::is_conditional_check_failed_exception) => {
                Err(StoreError::AlreadyExists)
            }
            Err(error) => Err(StoreError::PutItem(error)),
        }
    }

    async fn update(&self, access_key: &str, is_admin: bool) -> Result<AccessCode, StoreError> {
        let result = self
            .client
            .update_item()
            .table_name(&*self.table)
            .key(ACCESS_KEY, AttributeValue::S(access_key.to_string()))
            .update_expression("SET #is_admin = :is_admin")
            .condition_expression("attribute_exists(#access_key)")
            .expression_attribute_names("#access_key", ACCESS_KEY)
            .expression_attribute_names("#is_admin", IS_ADMIN)
            .expression_attribute_values(":is_admin", AttributeValue::Bool(is_admin))
            .return_values(ReturnValue::AllNew)
            .send()
            .await;

        let output = match result {
            Ok(output) => output,
            Err(error)
                if error.as_service_error().is_some_and(UpdateItemError::is_conditional_check_failed_exception) =>
            {
                return Err(StoreError::NotFound);
            }
            Err(error) => return Err(StoreError::UpdateItem(error)),
        };

        item_to_access_code(output.attributes().ok_or(ItemError::Missing(ACCESS_KEY))?).map_err(StoreError::from)
    }

    async fn delete(&self, access_key: &str) -> Result<(), StoreError> {
        let result = self
            .client
            .delete_item()
            .table_name(&*self.table)
            .key(ACCESS_KEY, AttributeValue::S(access_key.to_string()))
            .condition_expression("attribute_exists(#access_key)")
            .expression_attribute_names("#access_key", ACCESS_KEY)
            .send()
            .await;

        match result {
            Ok(_) => Ok(()),
            Err(error)
                if error.as_service_error().is_some_and(DeleteItemError::is_conditional_check_failed_exception) =>
            {
                Err(StoreError::NotFound)
            }
            Err(error) => Err(StoreError::DeleteItem(error)),
        }
    }
}

fn access_code_to_item(code: &AccessCode) -> HashMap<String, AttributeValue> {
    HashMap::from([
        (ACCESS_KEY.to_string(), AttributeValue::S(code.access_key.clone())),
        (IS_ADMIN.to_string(), AttributeValue::Bool(code.is_admin)),
        (CREATED_AT.to_string(), AttributeValue::S(format_timestamp(code.created_at))),
    ])
}

fn item_to_access_code(item: &HashMap<String, AttributeValue>) -> Result<AccessCode, ItemError> {
    Ok(AccessCode {
        access_key: string_attr(item, ACCESS_KEY)?,
        is_admin: bool_attr(item, IS_ADMIN)?,
        created_at: timestamp_attr(item, CREATED_AT)?,
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

fn timestamp_attr(item: &HashMap<String, AttributeValue>, name: &'static str) -> Result<OffsetDateTime, ItemError> {
    let value = string_attr(item, name)?;
    OffsetDateTime::parse(&value, &Rfc3339).map_err(|source| ItemError::Timestamp { name, value, source })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> AccessCode {
        AccessCode {
            access_key: "test-key".to_string(),
            is_admin: true,
            created_at: OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap(),
        }
    }

    #[test]
    fn item_round_trips() {
        let code = sample();
        assert_eq!(item_to_access_code(&access_code_to_item(&code)).unwrap(), code);
    }

    #[test]
    fn missing_attribute_is_an_error() {
        let mut item = access_code_to_item(&sample());
        item.remove(IS_ADMIN);
        assert!(matches!(item_to_access_code(&item), Err(ItemError::Missing(IS_ADMIN))));
    }

    #[test]
    fn wrong_typed_attribute_is_an_error() {
        let mut item = access_code_to_item(&sample());
        item.insert(IS_ADMIN.to_string(), AttributeValue::S("true".to_string()));
        assert!(matches!(item_to_access_code(&item), Err(ItemError::WrongType { name: IS_ADMIN, .. })));
    }

    #[test]
    fn unparseable_timestamp_is_an_error() {
        let mut item = access_code_to_item(&sample());
        item.insert(CREATED_AT.to_string(), AttributeValue::S("not a date".to_string()));
        assert!(matches!(item_to_access_code(&item), Err(ItemError::Timestamp { name: CREATED_AT, .. })));
    }
}
