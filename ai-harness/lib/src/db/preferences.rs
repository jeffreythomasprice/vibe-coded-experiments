//! DAL for `preferences` — a generic key-value store. `value` is opaque TEXT;
//! this module never interprets it. See `sql/0002_preferences.sql`'s doc.

use super::{now_iso8601, Db, Result};

/// The stored value for `key`, or `None` if no row exists yet. Absence is
/// normal — a missing key means "use the caller's default" — so this is never
/// `DbError::NotFound`.
pub async fn find(db: &Db, key: &str) -> Result<Option<String>> {
    let value: Option<String> = sqlx::query_scalar("SELECT value FROM preferences WHERE key = ?")
        .bind(key)
        .fetch_optional(db.pool())
        .await?;
    Ok(value)
}

/// Insert or replace the row for `key`.
pub async fn set(db: &Db, key: &str, value: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO preferences (key, value, updated_at)
         VALUES (?, ?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(key)
    .bind(value)
    .bind(now_iso8601())
    .execute(db.pool())
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn find_returns_none_for_a_missing_key() {
        let db = Db::in_memory().await.unwrap();
        assert_eq!(find(&db, "theme").await.unwrap(), None);
    }

    #[tokio::test]
    async fn set_then_find_round_trips() {
        let db = Db::in_memory().await.unwrap();
        set(&db, "theme", "dark").await.unwrap();
        assert_eq!(find(&db, "theme").await.unwrap(), Some("dark".to_string()));
    }

    #[tokio::test]
    async fn a_second_set_overwrites_rather_than_erroring() {
        let db = Db::in_memory().await.unwrap();
        set(&db, "theme", "dark").await.unwrap();
        set(&db, "theme", "light").await.unwrap();
        assert_eq!(find(&db, "theme").await.unwrap(), Some("light".to_string()));
    }

    #[tokio::test]
    async fn distinct_keys_do_not_collide() {
        let db = Db::in_memory().await.unwrap();
        set(&db, "theme", "dark").await.unwrap();
        set(&db, "other", "value").await.unwrap();
        assert_eq!(find(&db, "theme").await.unwrap(), Some("dark".to_string()));
        assert_eq!(find(&db, "other").await.unwrap(), Some("value".to_string()));
    }
}
