//! Conversation lifecycle (create/list/rename/delete) and assembling a full
//! [`ConversationView`] for display. Sending a message or approving a tool
//! call lives in [`super::chat`].

use shared::agent::TurnStop;
use shared::conversation::{ConversationSummary, ConversationView, ListConversations, PendingApproval};
use shared::ids::{AgentConfigId, ConversationId};

use crate::db;

use super::{Service, ServiceError};

impl Service {
    pub async fn list_conversations(
        &self,
        query: ListConversations,
    ) -> Result<Vec<ConversationSummary>, ServiceError> {
        Ok(db::conversations::list(&self.db, &query).await?)
    }

    pub async fn create_conversation(
        &self,
        agent_config_id: AgentConfigId,
        title: Option<String>,
    ) -> Result<ConversationSummary, ServiceError> {
        let agent = db::agents::get(&self.db, agent_config_id).await?;
        Ok(db::conversations::create(&self.db, &agent, title.as_deref()).await?)
    }

    pub async fn get_conversation(&self, id: ConversationId) -> Result<ConversationView, ServiceError> {
        let summary = db::conversations::summary(&self.db, id).await?;
        let agent = db::conversations::agent_config(&self.db, id).await?;
        let messages = db::conversations::load_messages(&self.db, id).await?;
        let pending = match db::turns::load_pending(&self.db, id).await? {
            Some((turn_id, turn)) => {
                let TurnStop::AwaitingApproval { pending, completed } = turn.stop else {
                    unreachable!(
                        "db::turns::load_pending only ever returns a turn whose stop \
                         is AwaitingApproval — see that function's doc"
                    );
                };
                Some(PendingApproval {
                    turn_id,
                    added: turn.added,
                    requests: pending,
                    completed,
                    usage: turn.usage,
                    steps: turn.steps,
                })
            }
            None => None,
        };
        Ok(ConversationView {
            summary,
            agent,
            messages,
            pending,
        })
    }

    pub async fn rename_conversation(&self, id: ConversationId, title: String) -> Result<(), ServiceError> {
        Ok(db::conversations::rename(&self.db, id, &title).await?)
    }

    /// Cascades to that conversation's turns, messages, and tool calls.
    pub async fn delete_conversation(&self, id: ConversationId) -> Result<(), ServiceError> {
        Ok(db::conversations::delete(&self.db, id).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::llm::router::Router;
    use shared::agent::AgentConfigInput;
    use shared::llm::model::ModelRef;
    use shared::llm::tool::Thinking;
    use std::sync::Arc;

    async fn service() -> Service {
        let db = Db::in_memory().await.unwrap();
        Service::new(db, Arc::new(Router::new()), Arc::new(crate::agent::ToolRegistry::new()))
    }

    fn input(name: &str) -> AgentConfigInput {
        AgentConfigInput {
            name: name.to_string(),
            description: None,
            model: ModelRef::new("scripted", "test-model"),
            system: vec![],
            max_tokens: 256,
            tools: vec![],
            tool_choice: None,
            thinking: Thinking::default(),
            stop_sequences: vec![],
            max_steps: 4,
        }
    }

    #[tokio::test]
    async fn create_then_get_conversation_has_no_pending_approval() {
        let service = service().await;
        let agent = service.create_agent(input("ops")).await.unwrap();
        let created = service.create_conversation(agent.id, Some("Deploy".to_string())).await.unwrap();

        let view = service.get_conversation(created.id).await.unwrap();
        assert_eq!(view.summary, created);
        assert_eq!(view.agent, agent);
        assert!(view.messages.is_empty());
        assert!(view.pending.is_none());
    }

    #[tokio::test]
    async fn rename_then_get_shows_the_new_title() {
        let service = service().await;
        let agent = service.create_agent(input("ops")).await.unwrap();
        let created = service.create_conversation(agent.id, None).await.unwrap();
        service.rename_conversation(created.id, "Renamed".to_string()).await.unwrap();
        let view = service.get_conversation(created.id).await.unwrap();
        assert_eq!(view.summary.title.as_deref(), Some("Renamed"));
    }

    #[tokio::test]
    async fn delete_conversation_removes_it_from_the_list() {
        let service = service().await;
        let agent = service.create_agent(input("ops")).await.unwrap();
        let created = service.create_conversation(agent.id, None).await.unwrap();
        service.delete_conversation(created.id).await.unwrap();
        let listed = service.list_conversations(ListConversations::default()).await.unwrap();
        assert!(listed.is_empty());
    }

    #[tokio::test]
    async fn get_conversation_on_a_missing_id_errors_not_found() {
        let service = service().await;
        let err = service.get_conversation(ConversationId(999)).await.unwrap_err();
        assert!(matches!(err, ServiceError::Db(crate::db::DbError::NotFound { .. })));
    }
}
