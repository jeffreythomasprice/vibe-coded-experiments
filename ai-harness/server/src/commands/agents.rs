//! CRUD on agent configs.

use lib::service::Service;
use shared::agent::{AgentConfig, AgentConfigInput, ToolSpec};
use shared::error::ErrorReport;
use shared::ids::AgentConfigId;

#[tauri::command]
pub async fn list_agents(service: tauri::State<'_, Service>) -> Result<Vec<AgentConfig>, ErrorReport> {
    service.list_agents().await.map_err(|err| (&err).into())
}

#[tauri::command]
pub async fn get_agent(
    service: tauri::State<'_, Service>,
    id: AgentConfigId,
) -> Result<AgentConfig, ErrorReport> {
    service.get_agent(id).await.map_err(|err| (&err).into())
}

/// Every tool this build knows how to execute — what a "pick your tools" UI
/// offers when building an [`AgentConfigInput`]. Sync: it only reads an
/// in-memory registry, no database or network involved.
#[tauri::command]
pub fn tool_catalog(service: tauri::State<'_, Service>) -> Vec<ToolSpec> {
    service.tool_catalog()
}

#[tauri::command]
pub async fn create_agent(
    service: tauri::State<'_, Service>,
    input: AgentConfigInput,
) -> Result<AgentConfig, ErrorReport> {
    service.create_agent(input).await.map_err(|err| (&err).into())
}

#[tauri::command]
pub async fn update_agent(
    service: tauri::State<'_, Service>,
    id: AgentConfigId,
    input: AgentConfigInput,
) -> Result<AgentConfig, ErrorReport> {
    service.update_agent(id, input).await.map_err(|err| (&err).into())
}

#[tauri::command]
pub async fn delete_agent(service: tauri::State<'_, Service>, id: AgentConfigId) -> Result<(), ErrorReport> {
    service.delete_agent(id).await.map_err(|err| (&err).into())
}
