//! DAL for `conversations`, plus reading back `messages`.
//!
//! Writing messages/turns lives in [`super::turns`] — this module is reads
//! plus the conversation row's own lifecycle (create/rename/delete).

use shared::agent::AgentConfig;
use shared::conversation::{ConversationSummary, ListConversations, StoredMessage};
use shared::ids::{AgentConfigId, ConversationId, MessageId, TurnId};
use shared::llm::message::{Conversation, ContentBlock, Message, Role};
use sqlx::sqlite::SqliteRow;
use sqlx::Row;

use super::error::DbError;
use super::{now_iso8601, Db, Result};

const ENTITY: &str = "conversation";

// Both queries below repeat the same column list rather than composing it at
// runtime: `sqlx::query` requires a `&'static str` (its SQL-injection lint
// rejects a `format!`-built `String`), so there is no cheap way to share it
// without reaching for `QueryBuilder`, which isn't worth it for two queries.
const SUMMARY_BY_ID_SQL: &str = "SELECT c.id, c.title, c.agent_config_id, c.agent_config_json, \
     c.created_at, c.updated_at, c.last_message_at, \
     EXISTS(SELECT 1 FROM turns t WHERE t.conversation_id = c.id AND t.state = 'awaiting_approval') AS awaiting_approval \
     FROM conversations c WHERE c.id = ?";

const LIST_SQL: &str = "SELECT c.id, c.title, c.agent_config_id, c.agent_config_json, \
     c.created_at, c.updated_at, c.last_message_at, \
     EXISTS(SELECT 1 FROM turns t WHERE t.conversation_id = c.id AND t.state = 'awaiting_approval') AS awaiting_approval \
     FROM conversations c \
     WHERE (? IS NULL OR c.title LIKE '%' || ? || '%') \
     ORDER BY COALESCE(c.last_message_at, c.updated_at) DESC, c.id DESC LIMIT ? OFFSET ?";

pub async fn create(db: &Db, agent: &AgentConfig, title: Option<&str>) -> Result<ConversationSummary> {
    let now = now_iso8601();
    let agent_config_json = serde_json::to_string(agent).expect("AgentConfig always serializes");
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO conversations (title, agent_config_id, agent_config_json, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?)
         RETURNING id",
    )
    .bind(title.map(str::to_string))
    .bind(agent.id.get())
    .bind(agent_config_json)
    .bind(now.clone())
    .bind(now.clone())
    .fetch_one(db.pool())
    .await?;

    Ok(ConversationSummary {
        id: ConversationId(id),
        title: title.map(str::to_string),
        agent_config_id: Some(agent.id),
        agent_name: agent.input.name.clone(),
        created_at: now.clone(),
        updated_at: now,
        last_message_at: None,
        awaiting_approval: false,
    })
}

pub async fn summary(db: &Db, id: ConversationId) -> Result<ConversationSummary> {
    let row = sqlx::query(SUMMARY_BY_ID_SQL)
        .bind(id.get())
        .fetch_optional(db.pool())
        .await?
        .ok_or_else(|| DbError::NotFound {
            entity: ENTITY,
            id: id.to_string(),
        })?;
    decode_summary(row)
}

/// The `AgentConfig` this conversation was started under, frozen at creation
/// time — see `sql/0001_init.sql`'s doc on `conversations.agent_config_json`
/// for why this is never a live join.
pub async fn agent_config(db: &Db, id: ConversationId) -> Result<AgentConfig> {
    let json: String = sqlx::query_scalar("SELECT agent_config_json FROM conversations WHERE id = ?")
        .bind(id.get())
        .fetch_optional(db.pool())
        .await?
        .ok_or_else(|| DbError::NotFound {
            entity: ENTITY,
            id: id.to_string(),
        })?;
    serde_json::from_str(&json).map_err(|source| DbError::Json {
        table: "conversations",
        column: "agent_config_json",
        entity: ENTITY,
        id: id.to_string(),
        source,
    })
}

pub async fn list(db: &Db, query: &ListConversations) -> Result<Vec<ConversationSummary>> {
    let limit = query.limit.unwrap_or(200).min(1000) as i64;
    let offset = query.offset.unwrap_or(0) as i64;
    let rows = sqlx::query(LIST_SQL)
        .bind(query.search.clone())
        .bind(query.search.clone())
        .bind(limit)
        .bind(offset)
        .fetch_all(db.pool())
        .await?;
    rows.into_iter().map(decode_summary).collect()
}

pub async fn rename(db: &Db, id: ConversationId, title: &str) -> Result<()> {
    let result = sqlx::query("UPDATE conversations SET title = ?, updated_at = ? WHERE id = ?")
        .bind(title.to_string())
        .bind(now_iso8601())
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

/// Cascades to `turns`, `messages`, and `tool_calls` via `ON DELETE CASCADE`.
pub async fn delete(db: &Db, id: ConversationId) -> Result<()> {
    let result = sqlx::query("DELETE FROM conversations WHERE id = ?")
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

pub async fn load_messages(db: &Db, id: ConversationId) -> Result<Vec<StoredMessage>> {
    let rows = sqlx::query(
        "SELECT id, seq, turn_id, role, content, created_at FROM messages \
         WHERE conversation_id = ? ORDER BY seq",
    )
    .bind(id.get())
    .fetch_all(db.pool())
    .await?;
    rows.into_iter().map(decode_message).collect()
}

/// The stored messages rebuilt into the bare `Conversation` shape a
/// `lib::agent::Agent` call takes. `system` is left `None` on purpose:
/// `Agent::run_loop` overwrites it from the agent's spec unconditionally, so
/// restoring it here would only create a second copy that could disagree.
pub async fn load_conversation(db: &Db, id: ConversationId) -> Result<Conversation> {
    let messages = load_messages(db, id).await?;
    Ok(Conversation {
        system: None,
        messages: messages
            .into_iter()
            .map(|m| Message {
                role: m.role,
                content: m.content,
            })
            .collect(),
    })
}

fn decode_summary(row: SqliteRow) -> Result<ConversationSummary> {
    let id: i64 = row.try_get("id")?;
    let title: Option<String> = row.try_get("title")?;
    let agent_config_id: Option<i64> = row.try_get("agent_config_id")?;
    let agent_config_json: String = row.try_get("agent_config_json")?;
    let created_at: String = row.try_get("created_at")?;
    let updated_at: String = row.try_get("updated_at")?;
    let last_message_at: Option<String> = row.try_get("last_message_at")?;
    let awaiting_approval: bool = row.try_get("awaiting_approval")?;

    let agent: AgentConfig = serde_json::from_str(&agent_config_json).map_err(|source| DbError::Json {
        table: "conversations",
        column: "agent_config_json",
        entity: ENTITY,
        id: id.to_string(),
        source,
    })?;

    Ok(ConversationSummary {
        id: ConversationId(id),
        title,
        agent_config_id: agent_config_id.map(AgentConfigId),
        agent_name: agent.input.name,
        created_at,
        updated_at,
        last_message_at,
        awaiting_approval,
    })
}

pub(crate) fn decode_message(row: SqliteRow) -> Result<StoredMessage> {
    let id: i64 = row.try_get("id")?;
    let seq: i64 = row.try_get("seq")?;
    let turn_id: Option<i64> = row.try_get("turn_id")?;
    let role_text: String = row.try_get("role")?;
    let content_json: String = row.try_get("content")?;
    let created_at: String = row.try_get("created_at")?;

    let role: Role = serde_json::from_value(serde_json::Value::String(role_text)).map_err(|source| {
        DbError::Json {
            table: "messages",
            column: "role",
            entity: "message",
            id: id.to_string(),
            source,
        }
    })?;
    let content: Vec<ContentBlock> = serde_json::from_str(&content_json).map_err(|source| DbError::Json {
        table: "messages",
        column: "content",
        entity: "message",
        id: id.to_string(),
        source,
    })?;

    Ok(StoredMessage {
        id: MessageId(id),
        seq: seq as u32,
        turn_id: turn_id.map(TurnId),
        role,
        content,
        created_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::agent::AgentConfigInput;
    use shared::llm::model::ModelRef;
    use shared::llm::tool::Thinking;

    async fn seed_agent(db: &Db) -> AgentConfig {
        super::super::agents::create(
            db,
            &AgentConfigInput {
                name: "assistant".to_string(),
                description: None,
                model: ModelRef::new("anthropic", "claude-opus-5"),
                system: vec![],
                max_tokens: 1024,
                tools: vec![],
                tool_choice: None,
                thinking: Thinking::default(),
                stop_sequences: vec![],
                max_steps: 8,
            },
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn create_then_summary_round_trips() {
        let db = Db::in_memory().await.unwrap();
        let agent = seed_agent(&db).await;
        let created = create(&db, &agent, Some("Deploy chat")).await.unwrap();
        let fetched = summary(&db, created.id).await.unwrap();
        assert_eq!(fetched, created);
        assert_eq!(fetched.agent_name, "assistant");
    }

    #[tokio::test]
    async fn agent_config_returns_the_frozen_snapshot_even_after_the_live_config_changes() {
        let db = Db::in_memory().await.unwrap();
        let agent = seed_agent(&db).await;
        let conv = create(&db, &agent, None).await.unwrap();

        let mut edited = agent.input.clone();
        edited.max_tokens = 4096;
        super::super::agents::update(&db, agent.id, &edited).await.unwrap();

        let frozen = agent_config(&db, conv.id).await.unwrap();
        assert_eq!(frozen.input.max_tokens, 1024, "conversation must keep its original snapshot");
    }

    #[tokio::test]
    async fn load_conversation_leaves_system_none() {
        let db = Db::in_memory().await.unwrap();
        let agent = seed_agent(&db).await;
        let conv = create(&db, &agent, None).await.unwrap();
        let conversation = load_conversation(&db, conv.id).await.unwrap();
        assert!(conversation.system.is_none());
        assert!(conversation.messages.is_empty());
    }

    #[tokio::test]
    async fn delete_cascades_to_messages_and_turns() {
        let db = Db::in_memory().await.unwrap();
        let agent = seed_agent(&db).await;
        let conv = create(&db, &agent, None).await.unwrap();

        let user_message = Message::user(vec![ContentBlock::Text {
            text: "hi".to_string(),
        }]);
        super::super::turns::begin(&db, conv.id, Some(&user_message)).await.unwrap();

        delete(&db, conv.id).await.unwrap();

        let messages = load_messages(&db, conv.id).await.unwrap();
        assert!(messages.is_empty());
        let turns_left: i64 = sqlx::query_scalar("SELECT count(*) FROM turns")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(turns_left, 0);
    }

    #[tokio::test]
    async fn rename_on_a_missing_id_errors_not_found() {
        let db = Db::in_memory().await.unwrap();
        let err = rename(&db, ConversationId(999), "x").await.unwrap_err();
        assert!(matches!(err, DbError::NotFound { .. }));
    }

    /// `last_message_at` is NULL until the first message, so a fresh
    /// conversation must not sort to the bottom of "most recently updated" —
    /// see the `LIST_SQL` doc on `COALESCE(last_message_at, updated_at)`.
    /// Timestamps are set directly via SQL rather than by waiting on the
    /// real clock `now_iso8601` reads from, so the ordering asserted here
    /// can't flake on a fast run where two calls land in the same
    /// millisecond.
    #[tokio::test]
    async fn list_falls_back_to_updated_at_when_last_message_at_is_null() {
        let db = Db::in_memory().await.unwrap();
        let agent = seed_agent(&db).await;

        let a = create(&db, &agent, Some("a")).await.unwrap();
        let b = create(&db, &agent, Some("b")).await.unwrap();
        let c = create(&db, &agent, Some("c")).await.unwrap();

        // a: updated long ago, never messaged.
        sqlx::query("UPDATE conversations SET updated_at = ? WHERE id = ?")
            .bind("2026-01-01T00:00:00.000Z")
            .bind(a.id.get())
            .execute(db.pool())
            .await
            .unwrap();
        // b: updated more recently than a, also never messaged.
        sqlx::query("UPDATE conversations SET updated_at = ? WHERE id = ?")
            .bind("2026-01-02T00:00:00.000Z")
            .bind(b.id.get())
            .execute(db.pool())
            .await
            .unwrap();
        // c: stale updated_at, but messaged after both a and b were updated —
        // last_message_at must win over updated_at for c, and over both a
        // and b overall.
        sqlx::query("UPDATE conversations SET updated_at = ?, last_message_at = ? WHERE id = ?")
            .bind("2026-01-01T00:00:00.000Z")
            .bind("2026-01-03T00:00:00.000Z")
            .bind(c.id.get())
            .execute(db.pool())
            .await
            .unwrap();

        let listed = list(&db, &ListConversations::default()).await.unwrap();
        let ids: Vec<ConversationId> = listed.iter().map(|s| s.id).collect();
        assert_eq!(ids, vec![c.id, b.id, a.id]);
    }

    #[tokio::test]
    async fn list_filters_by_title_search() {
        let db = Db::in_memory().await.unwrap();
        let agent = seed_agent(&db).await;
        create(&db, &agent, Some("Deploy prod")).await.unwrap();
        create(&db, &agent, Some("Chat about cats")).await.unwrap();

        let results = list(
            &db,
            &ListConversations {
                search: Some("deploy".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title.as_deref(), Some("Deploy prod"));
    }
}
