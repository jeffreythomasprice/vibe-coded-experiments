//! Generic key-value preferences (e.g. the preferred theme).

use lib::service::Service;
use shared::error::ErrorReport;

#[tauri::command]
pub async fn get_preference(
    service: tauri::State<'_, Service>,
    key: String,
) -> Result<Option<String>, ErrorReport> {
    service.preference(&key).await.map_err(|err| (&err).into())
}

#[tauri::command]
pub async fn set_preference(
    service: tauri::State<'_, Service>,
    key: String,
    value: String,
) -> Result<(), ErrorReport> {
    service.set_preference(&key, &value).await.map_err(|err| (&err).into())
}
