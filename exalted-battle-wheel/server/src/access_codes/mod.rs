//! The access-code store: what a code is, the storage-agnostic interface handlers and middleware
//! use, and the errors that can come out of it. `dynamo` is the real implementation; `memory` is
//! a test-only fake with the same conditional-write semantics.

mod dynamo;
#[cfg(test)]
mod memory;

pub use dynamo::connect;
#[cfg(test)]
pub use memory::MemoryAccessCodeStore;

use crate::dynamo_client::ItemError;
use aws_sdk_dynamodb::error::SdkError;
use aws_sdk_dynamodb::operation::delete_item::DeleteItemError;
use aws_sdk_dynamodb::operation::get_item::GetItemError;
use aws_sdk_dynamodb::operation::put_item::PutItemError;
use aws_sdk_dynamodb::operation::scan::ScanError;
use aws_sdk_dynamodb::operation::update_item::UpdateItemError;
pub use shared::access::AccessCode;
use std::future::Future;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("no such access code")]
    NotFound,
    #[error("access code already exists")]
    AlreadyExists,
    #[error(transparent)]
    Item(#[from] ItemError),
    #[error("dynamodb get_item failed")]
    GetItem(#[source] SdkError<GetItemError>),
    #[error("dynamodb put_item failed")]
    PutItem(#[source] SdkError<PutItemError>),
    #[error("dynamodb update_item failed")]
    UpdateItem(#[source] SdkError<UpdateItemError>),
    #[error("dynamodb delete_item failed")]
    DeleteItem(#[source] SdkError<DeleteItemError>),
    #[error("dynamodb scan failed")]
    Scan(#[source] SdkError<ScanError>),
}

pub trait AccessCodeStore: Clone + Send + Sync + 'static {
    fn get(&self, access_key: &str) -> impl Future<Output = Result<Option<AccessCode>, StoreError>> + Send;
    fn list(&self) -> impl Future<Output = Result<Vec<AccessCode>, StoreError>> + Send;
    /// `access_key`, if given, becomes the code verbatim; otherwise one is generated. A collision
    /// with an existing key -- whether given or generated -- is `StoreError::AlreadyExists`.
    fn create(
        &self,
        access_key: Option<&str>,
        is_admin: bool,
    ) -> impl Future<Output = Result<AccessCode, StoreError>> + Send;
    fn update(&self, access_key: &str, is_admin: bool) -> impl Future<Output = Result<AccessCode, StoreError>> + Send;
    fn delete(&self, access_key: &str) -> impl Future<Output = Result<(), StoreError>> + Send;
}

fn generate_key() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_keys_are_distinct() {
        assert_ne!(generate_key(), generate_key());
    }
}
