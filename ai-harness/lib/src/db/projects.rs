//! DAL for `projects`.
//!
//! Structurally mirrors [`super::themes`]: free functions over `&Db`,
//! `RETURNING id` on insert, `rows_affected() == 0` → `DbError::NotFound`,
//! a `decode(row)` helper mapping JSON failures to `DbError::Json`, and the
//! same `ensure_name_available`-inside-`write_tx` pattern for a typed
//! `DbError::ProjectNameTaken` ahead of the `projects_name` unique index.
//!
//! The one addition beyond that template: [`for_conversation`], which reads
//! `conversations.project_id` and resolves it to the project it currently
//! points at — `None` for both "never assigned" and "assigned project
//! since deleted" (`ON DELETE SET NULL` makes those indistinguishable here,
//! deliberately; see `sql/0004_projects.sql`). There is no frozen
//! `project_json` twin to fall back to, unlike `conversations.agent_config_json`
//! — see that file's doc for why a project is resolved live rather than
//! frozen.

use shared::ids::{ConversationId, ProjectId};
use shared::project::{Project, ProjectInput};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, Sqlite, Transaction};

use super::error::DbError;
use super::{now_iso8601, write_tx, Db, Result};

const ENTITY: &str = "project";

pub async fn create(db: &Db, input: &ProjectInput) -> Result<Project> {
    let mut tx = write_tx(db.pool()).await?;
    ensure_name_available(&mut tx, &input.name, None).await?;

    let now = now_iso8601();
    let dirs_json = serde_json::to_string(&input.dirs).expect("Vec<ProjectDir> always serializes");
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO projects (name, dirs_json, created_at, updated_at)
         VALUES (?, ?, ?, ?)
         RETURNING id",
    )
    .bind(&input.name)
    .bind(dirs_json)
    .bind(now.clone())
    .bind(now.clone())
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(Project {
        id: ProjectId::new(id),
        input: input.clone(),
        created_at: now.clone(),
        updated_at: now,
    })
}

pub async fn get(db: &Db, id: ProjectId) -> Result<Project> {
    find(db, id).await?.ok_or_else(|| DbError::NotFound {
        entity: ENTITY,
        id: id.to_string(),
    })
}

pub async fn find(db: &Db, id: ProjectId) -> Result<Option<Project>> {
    let row = sqlx::query("SELECT id, name, dirs_json, created_at, updated_at FROM projects WHERE id = ?")
        .bind(id.get())
        .fetch_optional(db.pool())
        .await?;
    row.map(decode).transpose()
}

pub async fn list(db: &Db) -> Result<Vec<Project>> {
    let rows = sqlx::query(
        "SELECT id, name, dirs_json, created_at, updated_at FROM projects ORDER BY name COLLATE NOCASE",
    )
    .fetch_all(db.pool())
    .await?;
    rows.into_iter().map(decode).collect()
}

pub async fn update(db: &Db, id: ProjectId, input: &ProjectInput) -> Result<Project> {
    let mut tx = write_tx(db.pool()).await?;
    ensure_name_available(&mut tx, &input.name, Some(id)).await?;

    let now = now_iso8601();
    let dirs_json = serde_json::to_string(&input.dirs).expect("Vec<ProjectDir> always serializes");
    let result = sqlx::query("UPDATE projects SET name = ?, dirs_json = ?, updated_at = ? WHERE id = ?")
        .bind(&input.name)
        .bind(dirs_json)
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
    let created_at: String = sqlx::query_scalar("SELECT created_at FROM projects WHERE id = ?")
        .bind(id.get())
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;

    Ok(Project {
        id,
        input: input.clone(),
        created_at,
        updated_at: now,
    })
}

pub async fn delete(db: &Db, id: ProjectId) -> Result<()> {
    let result = sqlx::query("DELETE FROM projects WHERE id = ?")
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

/// The project a conversation's `project_id` currently points at. `Ok(None)`
/// means "resolve to the default project" — covering both "never assigned
/// one" and "its project was deleted since"; see this module's doc and
/// `sql/0004_projects.sql`.
pub async fn for_conversation(db: &Db, id: ConversationId) -> Result<Option<Project>> {
    let project_id: Option<i64> = sqlx::query_scalar("SELECT project_id FROM conversations WHERE id = ?")
        .bind(id.get())
        .fetch_optional(db.pool())
        .await?
        .flatten();
    match project_id {
        Some(project_id) => find(db, ProjectId::new(project_id)).await,
        None => Ok(None),
    }
}

/// `name` free among *other* rows — `exclude` is the row being updated, so
/// renaming a project to its own current name is not a conflict. Runs
/// inside the same transaction as the write that follows, so the check and
/// the insert/update can't race.
async fn ensure_name_available(tx: &mut Transaction<'_, Sqlite>, name: &str, exclude: Option<ProjectId>) -> Result<()> {
    let taken: Option<i64> = match exclude {
        Some(id) => {
            sqlx::query_scalar("SELECT id FROM projects WHERE name = ? COLLATE NOCASE AND id != ?")
                .bind(name)
                .bind(id.get())
                .fetch_optional(&mut **tx)
                .await?
        }
        None => {
            sqlx::query_scalar("SELECT id FROM projects WHERE name = ? COLLATE NOCASE")
                .bind(name)
                .fetch_optional(&mut **tx)
                .await?
        }
    };
    if taken.is_some() {
        return Err(DbError::ProjectNameTaken { name: name.to_string() });
    }
    Ok(())
}

fn decode(row: SqliteRow) -> Result<Project> {
    let id: i64 = row.try_get("id")?;
    let name: String = row.try_get("name")?;
    let dirs_json: String = row.try_get("dirs_json")?;
    let created_at: String = row.try_get("created_at")?;
    let updated_at: String = row.try_get("updated_at")?;
    let dirs = serde_json::from_str(&dirs_json).map_err(|source| DbError::Json {
        table: "projects",
        column: "dirs_json",
        entity: ENTITY,
        id: id.to_string(),
        source,
    })?;
    Ok(Project {
        id: ProjectId::new(id),
        input: ProjectInput {
            name,
            description: None,
            dirs,
        },
        created_at,
        updated_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::project::{AccessMode, ProjectDir};
    use std::path::PathBuf;

    fn input(name: &str) -> ProjectInput {
        ProjectInput {
            name: name.to_string(),
            description: None,
            dirs: vec![ProjectDir {
                path: PathBuf::from("/tmp/some-project-dir"),
                mode: AccessMode::ReadWrite,
            }],
        }
    }

    #[tokio::test]
    async fn create_then_get_round_trips() {
        let db = Db::in_memory().await.unwrap();
        let created = create(&db, &input("webapp")).await.unwrap();
        let fetched = get(&db, created.id).await.unwrap();
        assert_eq!(fetched, created);
        assert_eq!(fetched.input.dirs.len(), 1);
    }

    #[tokio::test]
    async fn find_returns_none_for_a_missing_id() {
        let db = Db::in_memory().await.unwrap();
        assert!(find(&db, ProjectId::new(999)).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn get_errors_not_found_for_a_missing_id() {
        let db = Db::in_memory().await.unwrap();
        let err = get(&db, ProjectId::new(999)).await.unwrap_err();
        assert!(matches!(err, DbError::NotFound { entity: "project", .. }));
    }

    #[tokio::test]
    async fn list_orders_by_name_case_insensitively() {
        let db = Db::in_memory().await.unwrap();
        create(&db, &input("zebra")).await.unwrap();
        create(&db, &input("Apple")).await.unwrap();
        let names: Vec<String> = list(&db).await.unwrap().into_iter().map(|p| p.input.name).collect();
        assert_eq!(names, vec!["Apple".to_string(), "zebra".to_string()]);
    }

    #[tokio::test]
    async fn create_with_a_taken_name_is_rejected_case_insensitively() {
        let db = Db::in_memory().await.unwrap();
        create(&db, &input("Webapp")).await.unwrap();
        let err = create(&db, &input("WEBAPP")).await.unwrap_err();
        assert!(matches!(err, DbError::ProjectNameTaken { name } if name == "WEBAPP"));
    }

    #[tokio::test]
    async fn update_changes_fields_and_bumps_updated_at_but_not_created_at() {
        let db = Db::in_memory().await.unwrap();
        let created = create(&db, &input("webapp")).await.unwrap();

        let mut edited = input("renamed");
        edited.dirs.push(ProjectDir {
            path: PathBuf::from("/tmp/another-dir"),
            mode: AccessMode::ReadOnly,
        });
        let updated = update(&db, created.id, &edited).await.unwrap();

        assert_eq!(updated.input.name, "renamed");
        assert_eq!(updated.input.dirs.len(), 2);
        assert_eq!(updated.created_at, created.created_at);
    }

    #[tokio::test]
    async fn renaming_a_project_to_its_own_current_name_succeeds() {
        let db = Db::in_memory().await.unwrap();
        let created = create(&db, &input("webapp")).await.unwrap();
        let updated = update(&db, created.id, &input("webapp")).await.unwrap();
        assert_eq!(updated.input.name, "webapp");
    }

    #[tokio::test]
    async fn update_to_a_name_taken_by_another_project_is_rejected() {
        let db = Db::in_memory().await.unwrap();
        create(&db, &input("taken")).await.unwrap();
        let created = create(&db, &input("mine")).await.unwrap();
        let err = update(&db, created.id, &input("taken")).await.unwrap_err();
        assert!(matches!(err, DbError::ProjectNameTaken { .. }));
    }

    #[tokio::test]
    async fn update_on_a_missing_id_errors_not_found() {
        let db = Db::in_memory().await.unwrap();
        let err = update(&db, ProjectId::new(999), &input("ghost")).await.unwrap_err();
        assert!(matches!(err, DbError::NotFound { .. }));
    }

    #[tokio::test]
    async fn delete_removes_the_row() {
        let db = Db::in_memory().await.unwrap();
        let created = create(&db, &input("webapp")).await.unwrap();
        delete(&db, created.id).await.unwrap();
        assert!(find(&db, created.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn delete_on_a_missing_id_errors_not_found() {
        let db = Db::in_memory().await.unwrap();
        let err = delete(&db, ProjectId::new(999)).await.unwrap_err();
        assert!(matches!(err, DbError::NotFound { .. }));
    }

    #[tokio::test]
    async fn for_conversation_resolves_none_when_no_project_was_ever_assigned() {
        let db = Db::in_memory().await.unwrap();
        // Build a conversation the way `db::conversations::create` does,
        // without depending on that module here.
        let agent_config_json = "{}".to_string();
        let now = now_iso8601();
        let conversation_id: i64 = sqlx::query_scalar(
            "INSERT INTO conversations (title, agent_config_json, created_at, updated_at)
             VALUES (NULL, ?, ?, ?) RETURNING id",
        )
        .bind(agent_config_json)
        .bind(now.clone())
        .bind(now)
        .fetch_one(db.pool())
        .await
        .unwrap();

        let resolved = for_conversation(&db, ConversationId::new(conversation_id)).await.unwrap();
        assert!(resolved.is_none());
    }

    #[tokio::test]
    async fn for_conversation_resolves_none_after_its_project_is_deleted() {
        let db = Db::in_memory().await.unwrap();
        let project = create(&db, &input("webapp")).await.unwrap();
        let now = now_iso8601();
        let conversation_id: i64 = sqlx::query_scalar(
            "INSERT INTO conversations (title, agent_config_json, project_id, created_at, updated_at)
             VALUES (NULL, '{}', ?, ?, ?) RETURNING id",
        )
        .bind(project.id.get())
        .bind(now.clone())
        .bind(now)
        .fetch_one(db.pool())
        .await
        .unwrap();

        assert!(
            for_conversation(&db, ConversationId::new(conversation_id))
                .await
                .unwrap()
                .is_some()
        );

        delete(&db, project.id).await.unwrap();

        // ON DELETE SET NULL fires; the conversation now resolves to the
        // default project (fail closed to an empty virtual filesystem),
        // exactly as if no project had ever been assigned.
        let resolved = for_conversation(&db, ConversationId::new(conversation_id)).await.unwrap();
        assert!(resolved.is_none());
    }
}
