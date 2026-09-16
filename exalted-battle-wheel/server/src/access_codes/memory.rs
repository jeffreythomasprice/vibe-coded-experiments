//! An in-memory fake reproducing `DynamoAccessCodeStore`'s conditional-write semantics (`NotFound`
//! on updating/deleting a missing key, `AlreadyExists` on a colliding create), so router tests can
//! exercise real 401/403/200 behavior without Docker or DynamoDB.

use super::{generate_key, AccessCode, AccessCodeStore, StoreError};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use time::OffsetDateTime;

#[derive(Clone, Default)]
pub struct MemoryAccessCodeStore {
    codes: Arc<Mutex<HashMap<String, AccessCode>>>,
}

impl AccessCodeStore for MemoryAccessCodeStore {
    async fn get(&self, access_key: &str) -> Result<Option<AccessCode>, StoreError> {
        Ok(self.codes.lock().unwrap().get(access_key).cloned())
    }

    async fn list(&self) -> Result<Vec<AccessCode>, StoreError> {
        Ok(self.codes.lock().unwrap().values().cloned().collect())
    }

    async fn create(&self, is_admin: bool) -> Result<AccessCode, StoreError> {
        let code = AccessCode { access_key: generate_key()?, is_admin, created_at: OffsetDateTime::now_utc() };
        let mut codes = self.codes.lock().unwrap();
        if codes.contains_key(&code.access_key) {
            return Err(StoreError::AlreadyExists);
        }
        codes.insert(code.access_key.clone(), code.clone());
        Ok(code)
    }

    async fn update(&self, access_key: &str, is_admin: bool) -> Result<AccessCode, StoreError> {
        let mut codes = self.codes.lock().unwrap();
        let code = codes.get_mut(access_key).ok_or(StoreError::NotFound)?;
        code.is_admin = is_admin;
        Ok(code.clone())
    }

    async fn delete(&self, access_key: &str) -> Result<(), StoreError> {
        let mut codes = self.codes.lock().unwrap();
        codes.remove(access_key).map(|_| ()).ok_or(StoreError::NotFound)
    }
}

impl MemoryAccessCodeStore {
    /// Seeds a code directly, bypassing generation, so tests can log in as a known key.
    pub fn seed(&self, access_key: &str, is_admin: bool) -> AccessCode {
        let code = AccessCode { access_key: access_key.to_string(), is_admin, created_at: OffsetDateTime::now_utc() };
        self.codes.lock().unwrap().insert(code.access_key.clone(), code.clone());
        code
    }
}
