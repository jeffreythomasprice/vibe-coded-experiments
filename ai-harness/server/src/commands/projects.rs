//! CRUD on projects, the new-conversation picker listing, and the sandbox
//! status the settings page shows.

use std::sync::Arc;

use lib::service::Service;
use shared::error::ErrorReport;
use shared::ids::ProjectId;
use shared::project::{Project, ProjectInput, ProjectSnapshot, SandboxStatus};

#[tauri::command]
pub async fn list_projects(service: tauri::State<'_, Arc<Service>>) -> Result<Vec<Project>, ErrorReport> {
    service.list_projects().await.map_err(|err| (&err).into())
}

#[tauri::command]
pub async fn list_projects_for_picker(
    service: tauri::State<'_, Arc<Service>>,
) -> Result<Vec<ProjectSnapshot>, ErrorReport> {
    service.list_projects_for_picker().await.map_err(|err| (&err).into())
}

#[tauri::command]
pub async fn create_project(
    service: tauri::State<'_, Arc<Service>>,
    input: ProjectInput,
) -> Result<Project, ErrorReport> {
    service.create_project(input).await.map_err(|err| (&err).into())
}

#[tauri::command]
pub async fn update_project(
    service: tauri::State<'_, Arc<Service>>,
    id: ProjectId,
    input: ProjectInput,
) -> Result<Project, ErrorReport> {
    service.update_project(id, input).await.map_err(|err| (&err).into())
}

#[tauri::command]
pub async fn delete_project(service: tauri::State<'_, Arc<Service>>, id: ProjectId) -> Result<(), ErrorReport> {
    service.delete_project(id).await.map_err(|err| (&err).into())
}

#[tauri::command]
pub fn sandbox_status(service: tauri::State<'_, Arc<Service>>) -> SandboxStatus {
    service.sandbox_status()
}
