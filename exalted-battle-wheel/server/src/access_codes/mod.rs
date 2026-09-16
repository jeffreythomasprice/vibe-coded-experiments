//! The access-code store: what a code is, the storage-agnostic interface handlers and middleware
//! use, and the errors that can come out of it. `dynamo` is the real implementation; `memory` is
//! a test-only fake with the same conditional-write semantics.

mod dynamo;
#[cfg(test)]
mod memory;

pub use dynamo::connect;
#[cfg(test)]
pub use memory::MemoryAccessCodeStore;

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
    #[error("could not generate an access code: {0}")]
    Random(getrandom::Error),
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

#[derive(Debug, thiserror::Error)]
pub enum ItemError {
    #[error("item has no {0:?} attribute")]
    Missing(&'static str),
    #[error("item attribute {name:?} is not of type {expected}")]
    WrongType { name: &'static str, expected: &'static str },
    #[error("item attribute {name:?} is not an rfc3339 timestamp {value:?}: {source}")]
    Timestamp { name: &'static str, value: String, source: time::error::Parse },
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

// Crockford base32: no I, L, O or U, so a code can be read aloud or typed without ambiguity. 256
// is a multiple of 32, so `byte % 32` samples the alphabet with no modulo bias.
const CODE_ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const CODE_LENGTH: usize = 20;

fn generate_key() -> Result<String, StoreError> {
    let mut bytes = [0u8; CODE_LENGTH];
    getrandom::fill(&mut bytes).map_err(StoreError::Random)?;
    Ok(bytes.iter().map(|byte| char::from(CODE_ALPHABET[usize::from(*byte) % CODE_ALPHABET.len()])).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_keys_are_the_right_length_and_alphabet() {
        let key = generate_key().unwrap();
        assert_eq!(key.len(), CODE_LENGTH);
        assert!(key.bytes().all(|byte| CODE_ALPHABET.contains(&byte)));
    }

    #[test]
    fn generated_keys_are_distinct() {
        let a = generate_key().unwrap();
        let b = generate_key().unwrap();
        assert_ne!(a, b);
    }
}
