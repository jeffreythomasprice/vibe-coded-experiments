//! CRUD on user-authored theme documents.

use std::sync::Arc;

use lib::service::Service;
use shared::error::ErrorReport;
use shared::ids::ThemeId;
use shared::theme::{UserTheme, UserThemeInput};

#[tauri::command]
pub async fn list_themes(service: tauri::State<'_, Arc<Service>>) -> Result<Vec<UserTheme>, ErrorReport> {
    service.list_themes().await.map_err(|err| (&err).into())
}

#[tauri::command]
pub async fn create_theme(
    service: tauri::State<'_, Arc<Service>>,
    input: UserThemeInput,
) -> Result<UserTheme, ErrorReport> {
    service.create_theme(input).await.map_err(|err| (&err).into())
}

#[tauri::command]
pub async fn update_theme(
    service: tauri::State<'_, Arc<Service>>,
    id: ThemeId,
    input: UserThemeInput,
) -> Result<UserTheme, ErrorReport> {
    service.update_theme(id, input).await.map_err(|err| (&err).into())
}

#[tauri::command]
pub async fn delete_theme(service: tauri::State<'_, Arc<Service>>, id: ThemeId) -> Result<(), ErrorReport> {
    service.delete_theme(id).await.map_err(|err| (&err).into())
}
