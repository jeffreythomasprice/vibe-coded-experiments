//! Starting, driving, attaching to, and deleting conversations.

use std::sync::Arc;

use lib::service::Service;
use shared::agent::{AgentEvent, ToolDecision};
use shared::conversation::{ConversationSummary, ConversationView, ListConversations, RunStatus};
use shared::error::ErrorReport;
use shared::ids::{AgentConfigId, ConversationId};

#[tauri::command]
pub async fn list_conversations(
    service: tauri::State<'_, Arc<Service>>,
    query: ListConversations,
) -> Result<Vec<ConversationSummary>, ErrorReport> {
    service.list_conversations(query).await.map_err(|err| (&err).into())
}

#[tauri::command]
pub async fn create_conversation(
    service: tauri::State<'_, Arc<Service>>,
    agent_config_id: AgentConfigId,
    title: Option<String>,
) -> Result<ConversationSummary, ErrorReport> {
    service
        .create_conversation(agent_config_id, title)
        .await
        .map_err(|err| (&err).into())
}

#[tauri::command]
pub async fn get_conversation(
    service: tauri::State<'_, Arc<Service>>,
    id: ConversationId,
) -> Result<ConversationView, ErrorReport> {
    service.get_conversation(id).await.map_err(|err| (&err).into())
}

#[tauri::command]
pub async fn rename_conversation(
    service: tauri::State<'_, Arc<Service>>,
    id: ConversationId,
    title: String,
) -> Result<(), ErrorReport> {
    service.rename_conversation(id, title).await.map_err(|err| (&err).into())
}

#[tauri::command]
pub async fn delete_conversation(
    service: tauri::State<'_, Arc<Service>>,
    id: ConversationId,
) -> Result<(), ErrorReport> {
    service.delete_conversation(id).await.map_err(|err| (&err).into())
}

/// Reserves `conversation_id`'s run and drives the turn on a task detached
/// from this call, so it outlives the command — unlike the old
/// `send_message`, whose whole turn lived inside one IPC call's future and
/// vanished the moment the caller stopped awaiting it. Returns as soon as
/// the turn is accepted; a caller watches it happen via
/// [`attach_conversation`], not this call's return value.
#[tauri::command]
pub async fn start_message(
    service: tauri::State<'_, Arc<Service>>,
    conversation_id: ConversationId,
    text: String,
) -> Result<(), ErrorReport> {
    service.start_message(conversation_id, text).map_err(|err| (&err).into())
}

/// The `approve_tools` counterpart to [`start_message`].
#[tauri::command]
pub async fn start_approve_tools(
    service: tauri::State<'_, Arc<Service>>,
    conversation_id: ConversationId,
    decisions: Vec<ToolDecision>,
) -> Result<(), ErrorReport> {
    service
        .start_approve_tools(conversation_id, decisions)
        .map_err(|err| (&err).into())
}

#[tauri::command]
pub async fn cancel_pending(
    service: tauri::State<'_, Arc<Service>>,
    conversation_id: ConversationId,
) -> Result<(), ErrorReport> {
    service.cancel_pending(conversation_id).await.map_err(|err| (&err).into())
}

/// Replays whatever `conversation_id`'s run has already emitted over
/// `on_event`, then forwards live events until it settles — the call a
/// client makes on mount, and again on returning to a conversation it left
/// mid-stream, to pick a run back up without losing anything it missed.
///
/// A dead `on_event` (the webview navigated away, or closed, mid-call) stops
/// this call from following further; the turn itself is unaffected; it
/// keeps running and settles normally; see `Service::attach`'s doc. In that
/// case there is nothing left to deliver a status to — `RunStatus::Idle` is
/// a harmless filler for a caller that is already gone.
#[tauri::command]
pub async fn attach_conversation(
    service: tauri::State<'_, Arc<Service>>,
    conversation_id: ConversationId,
    on_event: tauri::ipc::Channel<AgentEvent>,
) -> Result<RunStatus, ErrorReport> {
    let status = service
        .attach(conversation_id, move |event| match on_event.send(event.clone()) {
            Ok(()) => true,
            Err(err) => {
                tracing::debug!(%err, "dropping agent event: channel closed");
                false
            }
        })
        .await;
    Ok(status.unwrap_or(RunStatus::Idle))
}
