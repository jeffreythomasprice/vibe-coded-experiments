//! Generic key-value preferences. This layer knows nothing about what any
//! particular key means — that decoding (e.g. `shared::theme::ThemeChoice`)
//! is the caller's job, same as `lib::db::preferences` leaves `value` opaque.

use crate::db;

use super::{Service, ServiceError};

impl Service {
    /// The stored value for `key`, or `None` if it has never been set.
    pub async fn preference(&self, key: &str) -> Result<Option<String>, ServiceError> {
        Ok(db::preferences::find(&self.db, key).await?)
    }

    pub async fn set_preference(&self, key: &str, value: &str) -> Result<(), ServiceError> {
        Ok(db::preferences::set(&self.db, key, value).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::llm::router::Router;
    use std::sync::Arc;

    async fn empty_service() -> Service {
        let db = Db::in_memory().await.unwrap();
        Service::new(db, Arc::new(Router::new()), Arc::new(crate::agent::ToolRegistry::new()))
    }

    #[tokio::test]
    async fn preference_is_none_for_a_key_that_was_never_set() {
        let service = empty_service().await;
        assert_eq!(service.preference("theme").await.unwrap(), None);
    }

    #[tokio::test]
    async fn set_preference_then_preference_round_trips() {
        let service = empty_service().await;
        service.set_preference("theme", "dark").await.unwrap();
        assert_eq!(service.preference("theme").await.unwrap(), Some("dark".to_string()));
    }
}
