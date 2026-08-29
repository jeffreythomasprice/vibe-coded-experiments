//! CRUD on agent configs.

use shared::agent::{AgentConfig, AgentConfigInput, ToolSpec};
use shared::ids::AgentConfigId;

use crate::db;

use super::{Service, ServiceError};

impl Service {
    pub async fn list_agents(&self) -> Result<Vec<AgentConfig>, ServiceError> {
        Ok(db::agents::list(&self.db).await?)
    }

    pub async fn get_agent(&self, id: AgentConfigId) -> Result<AgentConfig, ServiceError> {
        Ok(db::agents::get(&self.db, id).await?)
    }

    /// Every tool this build knows how to execute — what a "pick your
    /// tools" UI offers when building an [`AgentConfigInput`].
    pub fn tool_catalog(&self) -> Vec<ToolSpec> {
        self.tools.catalog()
    }

    /// Rejects a tool selection naming a tool this build doesn't have,
    /// before it's ever saved — better to fail here than to silently drop
    /// it the first time someone tries to run the agent.
    pub async fn create_agent(&self, input: AgentConfigInput) -> Result<AgentConfig, ServiceError> {
        self.tools.resolve(&input.tools)?;
        Ok(db::agents::create(&self.db, &input).await?)
    }

    pub async fn update_agent(&self, id: AgentConfigId, input: AgentConfigInput) -> Result<AgentConfig, ServiceError> {
        self.tools.resolve(&input.tools)?;
        Ok(db::agents::update(&self.db, id, &input).await?)
    }

    /// Conversations already started from this agent keep their own frozen
    /// snapshot and stay runnable — see `sql/0001_init.sql`'s doc on
    /// `conversations.agent_config_json`.
    pub async fn delete_agent(&self, id: AgentConfigId) -> Result<(), ServiceError> {
        Ok(db::agents::delete(&self.db, id).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tool::{tool, JsonSchema, ToolOutput};
    use crate::db::Db;
    use crate::llm::router::Router;
    use serde::Deserialize;
    use shared::llm::model::ModelRef;
    use shared::llm::tool::Thinking;
    use std::sync::Arc;

    #[derive(Deserialize, JsonSchema)]
    struct PingArgs {}

    async fn service_with_ping_tool() -> Service {
        let db = Db::in_memory().await.unwrap();
        let mut registry = crate::agent::ToolRegistry::new();
        registry.register(tool("ping", "Ping", |_args: PingArgs| async move {
            Ok::<_, anyhow::Error>(ToolOutput::from("pong"))
        }));
        Service::new(db, Arc::new(Router::new()), Arc::new(registry))
    }

    fn input(name: &str, tools: Vec<String>) -> AgentConfigInput {
        AgentConfigInput {
            name: name.to_string(),
            description: None,
            model: ModelRef::new("scripted", "test-model"),
            system: vec![],
            max_tokens: 256,
            tools,
            tool_choice: None,
            thinking: Thinking::default(),
            stop_sequences: vec![],
            max_steps: 4,
        }
    }

    #[tokio::test]
    async fn create_then_list_and_get_round_trip() {
        let service = service_with_ping_tool().await;
        let created = service.create_agent(input("ops", vec![])).await.unwrap();
        let listed = service.list_agents().await.unwrap();
        assert_eq!(listed, vec![created.clone()]);
        assert_eq!(service.get_agent(created.id).await.unwrap(), created);
    }

    #[tokio::test]
    async fn create_rejects_an_unknown_tool_before_saving_anything() {
        let service = service_with_ping_tool().await;
        let err = service
            .create_agent(input("ops", vec!["does-not-exist".to_string()]))
            .await
            .unwrap_err();
        assert!(matches!(err, ServiceError::Agent(_)));
        assert!(service.list_agents().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn create_accepts_a_known_tool() {
        let service = service_with_ping_tool().await;
        let created = service
            .create_agent(input("ops", vec!["ping".to_string()]))
            .await
            .unwrap();
        assert_eq!(created.input.tools, vec!["ping".to_string()]);
    }

    #[tokio::test]
    async fn tool_catalog_reports_the_registered_tools() {
        let service = service_with_ping_tool().await;
        let catalog = service.tool_catalog();
        assert_eq!(catalog.len(), 1);
        assert_eq!(catalog[0].def.name, "ping");
    }

    #[tokio::test]
    async fn delete_agent_removes_it() {
        let service = service_with_ping_tool().await;
        let created = service.create_agent(input("ops", vec![])).await.unwrap();
        service.delete_agent(created.id).await.unwrap();
        assert!(service.list_agents().await.unwrap().is_empty());
    }
}
