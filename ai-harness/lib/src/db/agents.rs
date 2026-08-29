//! DAL for `agent_configs`.

use shared::agent::{AgentConfig, AgentConfigInput};
use shared::ids::AgentConfigId;
use sqlx::sqlite::SqliteRow;
use sqlx::Row;

use super::error::DbError;
use super::{now_iso8601, Db, Result};

const ENTITY: &str = "agent_config";

pub async fn create(db: &Db, input: &AgentConfigInput) -> Result<AgentConfig> {
    let now = now_iso8601();
    let config_json = serde_json::to_string(input).expect("AgentConfigInput always serializes");
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO agent_configs (name, config_json, provider, model, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?)
         RETURNING id",
    )
    .bind(input.name.clone())
    .bind(config_json)
    .bind(input.model.provider.clone())
    .bind(input.model.model.clone())
    .bind(now.clone())
    .bind(now.clone())
    .fetch_one(db.pool())
    .await?;

    Ok(AgentConfig {
        id: AgentConfigId(id),
        input: input.clone(),
        created_at: now.clone(),
        updated_at: now,
    })
}

pub async fn get(db: &Db, id: AgentConfigId) -> Result<AgentConfig> {
    find(db, id).await?.ok_or_else(|| DbError::NotFound {
        entity: ENTITY,
        id: id.to_string(),
    })
}

pub async fn find(db: &Db, id: AgentConfigId) -> Result<Option<AgentConfig>> {
    let row = sqlx::query("SELECT id, config_json, created_at, updated_at FROM agent_configs WHERE id = ?")
        .bind(id.get())
        .fetch_optional(db.pool())
        .await?;
    row.map(decode).transpose()
}

pub async fn list(db: &Db) -> Result<Vec<AgentConfig>> {
    let rows = sqlx::query("SELECT id, config_json, created_at, updated_at FROM agent_configs ORDER BY name")
        .fetch_all(db.pool())
        .await?;
    rows.into_iter().map(decode).collect()
}

pub async fn update(db: &Db, id: AgentConfigId, input: &AgentConfigInput) -> Result<AgentConfig> {
    let now = now_iso8601();
    let config_json = serde_json::to_string(input).expect("AgentConfigInput always serializes");
    let result = sqlx::query(
        "UPDATE agent_configs
            SET name = ?, config_json = ?, provider = ?, model = ?, updated_at = ?
          WHERE id = ?",
    )
    .bind(input.name.clone())
    .bind(config_json)
    .bind(input.model.provider.clone())
    .bind(input.model.model.clone())
    .bind(now.clone())
    .bind(id.get())
    .execute(db.pool())
    .await?;
    if result.rows_affected() == 0 {
        return Err(DbError::NotFound {
            entity: ENTITY,
            id: id.to_string(),
        });
    }
    let created_at: String = sqlx::query_scalar("SELECT created_at FROM agent_configs WHERE id = ?")
        .bind(id.get())
        .fetch_one(db.pool())
        .await?;
    Ok(AgentConfig {
        id,
        input: input.clone(),
        created_at,
        updated_at: now,
    })
}

/// Conversations started from this agent keep their frozen snapshot and stay
/// runnable — `conversations.agent_config_id` is nulled by `ON DELETE SET
/// NULL`, but `conversations.agent_config_json` is untouched.
pub async fn delete(db: &Db, id: AgentConfigId) -> Result<()> {
    let result = sqlx::query("DELETE FROM agent_configs WHERE id = ?")
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

fn decode(row: SqliteRow) -> Result<AgentConfig> {
    let id: i64 = row.try_get("id")?;
    let config_json: String = row.try_get("config_json")?;
    let created_at: String = row.try_get("created_at")?;
    let updated_at: String = row.try_get("updated_at")?;
    let input: AgentConfigInput = serde_json::from_str(&config_json).map_err(|source| DbError::Json {
        table: "agent_configs",
        column: "config_json",
        entity: ENTITY,
        id: id.to_string(),
        source,
    })?;
    Ok(AgentConfig {
        id: AgentConfigId(id),
        input,
        created_at,
        updated_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::llm::model::ModelRef;
    use shared::llm::tool::Thinking;

    fn input(name: &str) -> AgentConfigInput {
        AgentConfigInput {
            name: name.to_string(),
            description: Some("A test agent".to_string()),
            model: ModelRef::new("anthropic", "claude-opus-5"),
            system: vec!["Be helpful.".to_string()],
            max_tokens: 1024,
            tools: vec!["deploy".to_string()],
            tool_choice: None,
            thinking: Thinking::default(),
            stop_sequences: vec![],
            max_steps: 8,
        }
    }

    #[tokio::test]
    async fn create_then_get_round_trips_the_full_input() {
        let db = Db::in_memory().await.unwrap();
        let created = create(&db, &input("ops")).await.unwrap();
        let fetched = get(&db, created.id).await.unwrap();
        assert_eq!(fetched, created);
        assert_eq!(fetched.input.tools, vec!["deploy".to_string()]);
    }

    #[tokio::test]
    async fn find_returns_none_for_a_missing_id() {
        let db = Db::in_memory().await.unwrap();
        assert!(find(&db, AgentConfigId(999)).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn get_errors_not_found_for_a_missing_id() {
        let db = Db::in_memory().await.unwrap();
        let err = get(&db, AgentConfigId(999)).await.unwrap_err();
        assert!(matches!(err, DbError::NotFound { entity: "agent_config", .. }));
    }

    #[tokio::test]
    async fn list_orders_by_name() {
        let db = Db::in_memory().await.unwrap();
        create(&db, &input("zebra")).await.unwrap();
        create(&db, &input("apple")).await.unwrap();
        let names: Vec<String> = list(&db).await.unwrap().into_iter().map(|c| c.input.name).collect();
        assert_eq!(names, vec!["apple".to_string(), "zebra".to_string()]);
    }

    #[tokio::test]
    async fn duplicate_names_are_rejected() {
        let db = Db::in_memory().await.unwrap();
        create(&db, &input("ops")).await.unwrap();
        let err = create(&db, &input("ops")).await.unwrap_err();
        assert!(matches!(err, DbError::Sql(_)));
    }

    #[tokio::test]
    async fn update_changes_fields_and_bumps_updated_at_but_not_created_at() {
        let db = Db::in_memory().await.unwrap();
        let created = create(&db, &input("ops")).await.unwrap();

        let mut edited = input("ops-renamed");
        edited.max_tokens = 2048;
        let updated = update(&db, created.id, &edited).await.unwrap();

        assert_eq!(updated.input.name, "ops-renamed");
        assert_eq!(updated.input.max_tokens, 2048);
        assert_eq!(updated.created_at, created.created_at);
    }

    #[tokio::test]
    async fn update_on_a_missing_id_errors_not_found() {
        let db = Db::in_memory().await.unwrap();
        let err = update(&db, AgentConfigId(999), &input("ghost")).await.unwrap_err();
        assert!(matches!(err, DbError::NotFound { .. }));
    }

    #[tokio::test]
    async fn delete_removes_the_row() {
        let db = Db::in_memory().await.unwrap();
        let created = create(&db, &input("ops")).await.unwrap();
        delete(&db, created.id).await.unwrap();
        assert!(find(&db, created.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn delete_on_a_missing_id_errors_not_found() {
        let db = Db::in_memory().await.unwrap();
        let err = delete(&db, AgentConfigId(999)).await.unwrap_err();
        assert!(matches!(err, DbError::NotFound { .. }));
    }
}
