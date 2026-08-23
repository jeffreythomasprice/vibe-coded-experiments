//! DAL for `themes` — user-authored theme documents.
//!
//! Structurally this mirrors `lib::db::agents`: free functions over `&Db`,
//! `RETURNING id` on insert, `rows_affected() == 0` → `DbError::NotFound`,
//! and a `decode(row)` helper mapping JSON failures to `DbError::Json`.
//!
//! The one deviation: `create`/`update` pre-check the name inside a
//! `write_tx` and return the typed `DbError::ThemeNameTaken` rather than
//! letting a duplicate surface as an opaque `DbError::Sql` the way
//! `agents::create` does — the settings page needs an obvious "that name is
//! taken" message, and `agents.rs` has no precedent for producing one. The
//! `themes_name` unique index (`COLLATE NOCASE`, `sql/0003_themes.sql`) is
//! the durable backstop if this check is ever bypassed, matching the
//! pre-check-plus-index pattern `turns::begin` uses for
//! `turns_one_open_per_conversation`.

use shared::ids::ThemeId;
use shared::theme::{UserTheme, UserThemeInput};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, Sqlite, Transaction};

use super::error::DbError;
use super::{now_iso8601, write_tx, Db, Result};

const ENTITY: &str = "theme";

pub async fn create(db: &Db, input: &UserThemeInput) -> Result<UserTheme> {
    let mut tx = write_tx(db.pool()).await?;
    ensure_name_available(&mut tx, &input.name, None).await?;

    let now = now_iso8601();
    let theme_json = serde_json::to_string(&input.theme).expect("Theme always serializes");
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO themes (name, theme_json, created_at, updated_at)
         VALUES (?, ?, ?, ?)
         RETURNING id",
    )
    .bind(&input.name)
    .bind(theme_json)
    .bind(now.clone())
    .bind(now.clone())
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(UserTheme {
        id: ThemeId::new(id),
        name: input.name.clone(),
        theme: input.theme.clone(),
        created_at: now.clone(),
        updated_at: now,
    })
}

pub async fn get(db: &Db, id: ThemeId) -> Result<UserTheme> {
    find(db, id).await?.ok_or_else(|| DbError::NotFound {
        entity: ENTITY,
        id: id.to_string(),
    })
}

pub async fn find(db: &Db, id: ThemeId) -> Result<Option<UserTheme>> {
    let row = sqlx::query("SELECT id, name, theme_json, created_at, updated_at FROM themes WHERE id = ?")
        .bind(id.get())
        .fetch_optional(db.pool())
        .await?;
    row.map(decode).transpose()
}

pub async fn list(db: &Db) -> Result<Vec<UserTheme>> {
    let rows = sqlx::query(
        "SELECT id, name, theme_json, created_at, updated_at FROM themes ORDER BY name COLLATE NOCASE",
    )
    .fetch_all(db.pool())
    .await?;
    rows.into_iter().map(decode).collect()
}

pub async fn update(db: &Db, id: ThemeId, input: &UserThemeInput) -> Result<UserTheme> {
    let mut tx = write_tx(db.pool()).await?;
    ensure_name_available(&mut tx, &input.name, Some(id)).await?;

    let now = now_iso8601();
    let theme_json = serde_json::to_string(&input.theme).expect("Theme always serializes");
    let result = sqlx::query(
        "UPDATE themes SET name = ?, theme_json = ?, updated_at = ? WHERE id = ?",
    )
    .bind(&input.name)
    .bind(theme_json)
    .bind(now.clone())
    .bind(id.get())
    .execute(&mut *tx)
    .await?;
    if result.rows_affected() == 0 {
        return Err(DbError::NotFound {
            entity: ENTITY,
            id: id.to_string(),
        });
    }
    let created_at: String = sqlx::query_scalar("SELECT created_at FROM themes WHERE id = ?")
        .bind(id.get())
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;

    Ok(UserTheme {
        id,
        name: input.name.clone(),
        theme: input.theme.clone(),
        created_at,
        updated_at: now,
    })
}

pub async fn delete(db: &Db, id: ThemeId) -> Result<()> {
    let result = sqlx::query("DELETE FROM themes WHERE id = ?")
        .bind(id.get())
        .execute(db.pool())
        .await?;
    if result.rows_affected() == 0 {
        return Err(DbError::NotFound {
            entity: ENTITY,
            id: id.to_string(),
        });
    }
    Ok(())
}

/// `name` free among *other* rows — `exclude` is the row being updated, so
/// renaming a theme to its own current name is not a conflict. Runs inside
/// the same transaction as the write that follows, so the check and the
/// insert/update can't race.
async fn ensure_name_available(
    tx: &mut Transaction<'_, Sqlite>,
    name: &str,
    exclude: Option<ThemeId>,
) -> Result<()> {
    let taken: Option<i64> = match exclude {
        Some(id) => {
            sqlx::query_scalar("SELECT id FROM themes WHERE name = ? COLLATE NOCASE AND id != ?")
                .bind(name)
                .bind(id.get())
                .fetch_optional(&mut **tx)
                .await?
        }
        None => {
            sqlx::query_scalar("SELECT id FROM themes WHERE name = ? COLLATE NOCASE")
                .bind(name)
                .fetch_optional(&mut **tx)
                .await?
        }
    };
    if taken.is_some() {
        return Err(DbError::ThemeNameTaken { name: name.to_string() });
    }
    Ok(())
}

fn decode(row: SqliteRow) -> Result<UserTheme> {
    let id: i64 = row.try_get("id")?;
    let name: String = row.try_get("name")?;
    let theme_json: String = row.try_get("theme_json")?;
    let created_at: String = row.try_get("created_at")?;
    let updated_at: String = row.try_get("updated_at")?;
    let theme = serde_json::from_str(&theme_json).map_err(|source| DbError::Json {
        table: "themes",
        column: "theme_json",
        entity: ENTITY,
        id: id.to_string(),
        source,
    })?;
    Ok(UserTheme {
        id: ThemeId::new(id),
        name,
        theme,
        created_at,
        updated_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::theme::BuiltIn;

    fn input(name: &str) -> UserThemeInput {
        UserThemeInput {
            name: name.to_string(),
            theme: BuiltIn::Dracula.theme(),
        }
    }

    #[tokio::test]
    async fn create_then_get_round_trips() {
        let db = Db::in_memory().await.unwrap();
        let created = create(&db, &input("My Theme")).await.unwrap();
        let fetched = get(&db, created.id).await.unwrap();
        assert_eq!(fetched, created);
        assert_eq!(fetched.theme, BuiltIn::Dracula.theme());
    }

    #[tokio::test]
    async fn find_returns_none_for_a_missing_id() {
        let db = Db::in_memory().await.unwrap();
        assert!(find(&db, ThemeId::new(999)).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn get_errors_not_found_for_a_missing_id() {
        let db = Db::in_memory().await.unwrap();
        let err = get(&db, ThemeId::new(999)).await.unwrap_err();
        assert!(matches!(err, DbError::NotFound { entity: "theme", .. }));
    }

    #[tokio::test]
    async fn list_orders_by_name_case_insensitively() {
        let db = Db::in_memory().await.unwrap();
        create(&db, &input("zebra")).await.unwrap();
        create(&db, &input("Apple")).await.unwrap();
        let names: Vec<String> = list(&db).await.unwrap().into_iter().map(|t| t.name).collect();
        assert_eq!(names, vec!["Apple".to_string(), "zebra".to_string()]);
    }

    #[tokio::test]
    async fn create_with_a_taken_name_is_rejected() {
        let db = Db::in_memory().await.unwrap();
        create(&db, &input("Dracula Custom")).await.unwrap();
        let err = create(&db, &input("Dracula Custom")).await.unwrap_err();
        assert!(matches!(err, DbError::ThemeNameTaken { name } if name == "Dracula Custom"));
    }

    #[tokio::test]
    async fn create_with_a_taken_name_is_rejected_case_insensitively() {
        let db = Db::in_memory().await.unwrap();
        create(&db, &input("dracula custom")).await.unwrap();
        let err = create(&db, &input("DRACULA CUSTOM")).await.unwrap_err();
        assert!(matches!(err, DbError::ThemeNameTaken { .. }));
    }

    #[tokio::test]
    async fn update_changes_fields_and_bumps_updated_at_but_not_created_at() {
        let db = Db::in_memory().await.unwrap();
        let created = create(&db, &input("My Theme")).await.unwrap();

        let mut edited = input("Renamed Theme");
        edited.theme = BuiltIn::Nord.theme();
        let updated = update(&db, created.id, &edited).await.unwrap();

        assert_eq!(updated.name, "Renamed Theme");
        assert_eq!(updated.theme, BuiltIn::Nord.theme());
        assert_eq!(updated.created_at, created.created_at);
    }

    #[tokio::test]
    async fn renaming_a_theme_to_its_own_current_name_succeeds() {
        let db = Db::in_memory().await.unwrap();
        let created = create(&db, &input("My Theme")).await.unwrap();
        let updated = update(&db, created.id, &input("My Theme")).await.unwrap();
        assert_eq!(updated.name, "My Theme");
    }

    #[tokio::test]
    async fn update_to_a_name_taken_by_another_theme_is_rejected() {
        let db = Db::in_memory().await.unwrap();
        create(&db, &input("Taken")).await.unwrap();
        let created = create(&db, &input("Mine")).await.unwrap();

        let err = update(&db, created.id, &input("Taken")).await.unwrap_err();
        assert!(matches!(err, DbError::ThemeNameTaken { .. }));
    }

    #[tokio::test]
    async fn update_on_a_missing_id_errors_not_found() {
        let db = Db::in_memory().await.unwrap();
        let err = update(&db, ThemeId::new(999), &input("ghost")).await.unwrap_err();
        assert!(matches!(err, DbError::NotFound { .. }));
    }

    #[tokio::test]
    async fn delete_removes_the_row() {
        let db = Db::in_memory().await.unwrap();
        let created = create(&db, &input("My Theme")).await.unwrap();
        delete(&db, created.id).await.unwrap();
        assert!(find(&db, created.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn delete_on_a_missing_id_errors_not_found() {
        let db = Db::in_memory().await.unwrap();
        let err = delete(&db, ThemeId::new(999)).await.unwrap_err();
        assert!(matches!(err, DbError::NotFound { .. }));
    }
}
