//! Conversation lifecycle (create/list/rename/delete) and assembling a full
//! [`ConversationView`] for display. Sending a message or approving a tool
//! call lives in [`super::chat`].

use shared::agent::TurnStop;
use shared::conversation::{ConversationSummary, ConversationView, ListConversations, PendingApproval, RunStatus};
use shared::ids::{AgentConfigId, ConversationId};
use shared::project::ProjectRef;

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
        project: ProjectRef,
        title: Option<String>,
    ) -> Result<ConversationSummary, ServiceError> {
        let agent = db::agents::get(&self.db, agent_config_id).await?;
        let project_id = self.resolve_project_id(project).await?;
        Ok(db::conversations::create(&self.db, &agent, project_id, title.as_deref()).await?)
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
        let decisions = db::turns::decisions_for_conversation(&self.db, id).await?;
        Ok(ConversationView {
            summary,
            agent,
            messages,
            pending,
            decisions,
        })
    }

    pub async fn rename_conversation(&self, id: ConversationId, title: String) -> Result<(), ServiceError> {
        Ok(db::conversations::rename(&self.db, id, &title).await?)
    }

    /// Cascades to that conversation's turns, messages, and tool calls. A
    /// turn still in flight is cancelled first: it holds this conversation's
    /// lock for its whole span, and its tools would otherwise keep running
    /// against a row that no longer exists.
    pub async fn delete_conversation(&self, id: ConversationId) -> Result<(), ServiceError> {
        if let Some(task) = self.runs.cancel(id, RunStatus::Cancelled) {
            // `abort` only schedules the driving future for dropping;
            // awaiting the handle (which resolves `Err(JoinError::is_cancelled)`)
            // is what makes that drop — and with it, the conversation lock
            // the turn was holding being released — observable here instead
            // of at some later, unknown point.
            let _ = task.await;
        }
        let _guard = self.try_lock_conversation(id)?;
        // Holding the guard across the delete is what guarantees the `runs`
        // and `locks` entries for `id` are gone before the row — and
        // therefore the rowid, since `conversations.id` has no
        // `AUTOINCREMENT` (see `shared::ids`) — is freed for reuse.
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
        let created = service.create_conversation(agent.id, ProjectRef::Default, Some("Deploy".to_string())).await.unwrap();

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
        let created = service.create_conversation(agent.id, ProjectRef::Default, None).await.unwrap();
        service.rename_conversation(created.id, "Renamed".to_string()).await.unwrap();
        let view = service.get_conversation(created.id).await.unwrap();
        assert_eq!(view.summary.title.as_deref(), Some("Renamed"));
    }

    #[tokio::test]
    async fn delete_conversation_removes_it_from_the_list() {
        let service = service().await;
        let agent = service.create_agent(input("ops")).await.unwrap();
        let created = service.create_conversation(agent.id, ProjectRef::Default, None).await.unwrap();
        service.delete_conversation(created.id).await.unwrap();
        let listed = service.list_conversations(ListConversations::default()).await.unwrap();
        assert!(listed.is_empty());
    }

    #[tokio::test]
    async fn delete_conversation_cancels_an_in_flight_run() {
        let service = service().await;
        let agent = service.create_agent(input("ops")).await.unwrap();
        let created = service.create_conversation(agent.id, ProjectRef::Default, None).await.unwrap();

        // Register a run "by hand," the same way `Service::start_message`
        // does, without a real provider round trip: a task that never
        // resolves on its own stands in for a turn still streaming.
        let _run = service.runs.begin(created.id).unwrap();
        let task = tokio::spawn(std::future::pending::<()>());
        service.runs.attach_task(created.id, task);

        service.delete_conversation(created.id).await.unwrap();

        assert!(service.runs.get(created.id).is_none(), "the run must be deregistered");
        let listed = service.list_conversations(ListConversations::default()).await.unwrap();
        assert!(listed.is_empty());
    }

    #[tokio::test]
    async fn delete_conversation_awaits_the_cancelled_task_before_taking_the_lock() {
        let service = service().await;
        let agent = service.create_agent(input("ops")).await.unwrap();
        let created = service.create_conversation(agent.id, ProjectRef::Default, None).await.unwrap();

        // A real in-flight turn holds this guard for its whole span (see
        // `chat::Service::send_message`). Move it into a task that never
        // resolves, so the guard is released only when that task's future
        // is actually dropped.
        let _run = service.runs.begin(created.id).unwrap();
        let guard = service.try_lock_conversation(created.id).unwrap();
        let task = tokio::spawn(async move {
            let _guard = guard;
            std::future::pending::<()>().await
        });
        service.runs.attach_task(created.id, task);

        // If `delete_conversation` tried to acquire the lock before the
        // cancelled task's future actually finished dropping (releasing the
        // guard), this would fail with `ConversationBusy` instead.
        service.delete_conversation(created.id).await.unwrap();

        // The lock is free again — nothing left over that could confuse the
        // next conversation to reuse this rowid.
        let _guard = service.try_lock_conversation(created.id).unwrap();
    }

    #[tokio::test]
    async fn get_conversation_on_a_missing_id_errors_not_found() {
        let service = service().await;
        let err = service.get_conversation(ConversationId(999)).await.unwrap_err();
        assert!(matches!(err, ServiceError::Db(crate::db::DbError::NotFound { .. })));
    }
}
