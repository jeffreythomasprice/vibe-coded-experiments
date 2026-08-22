//! Starting, driving, and deleting conversations.

use lib::service::{EventSink, Service};
use shared::agent::{AgentEvent, ToolDecision};
use shared::conversation::{ConversationSummary, ConversationView, ListConversations, TurnOutcome};
use shared::error::ErrorReport;
use shared::ids::{AgentConfigId, ConversationId};

/// Forwards every `AgentEvent` a turn produces over a Tauri IPC channel —
/// the bridge `shared::agent::event`'s module doc anticipated between the
/// agent loop and the frontend.
struct ChannelSink(tauri::ipc::Channel<AgentEvent>);

impl EventSink for ChannelSink {
    /// A send failure means the webview dropped the channel — the user
    /// navigated away, or closed the window. Logged, never propagated: the
    /// turn is already in flight and its result still has to be persisted;
    /// aborting here to save a UI update nobody is listening for would
    /// throw the model's answer away instead of just failing to show it.
    fn emit(&self, event: &AgentEvent) {
        if let Err(err) = self.0.send(event.clone()) {
            tracing::debug!(%err, "dropping agent event: channel closed");
        }
    }
}

#[tauri::command]
pub async fn list_conversations(
    service: tauri::State<'_, Service>,
    query: ListConversations,
) -> Result<Vec<ConversationSummary>, ErrorReport> {
    service.list_conversations(query).await.map_err(|err| (&err).into())
}

#[tauri::command]
pub async fn create_conversation(
    service: tauri::State<'_, Service>,
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
    service: tauri::State<'_, Service>,
    id: ConversationId,
) -> Result<ConversationView, ErrorReport> {
    service.get_conversation(id).await.map_err(|err| (&err).into())
}

#[tauri::command]
pub async fn rename_conversation(
    service: tauri::State<'_, Service>,
    id: ConversationId,
    title: String,
) -> Result<(), ErrorReport> {
    service.rename_conversation(id, title).await.map_err(|err| (&err).into())
}

#[tauri::command]
pub async fn delete_conversation(service: tauri::State<'_, Service>, id: ConversationId) -> Result<(), ErrorReport> {
    service.delete_conversation(id).await.map_err(|err| (&err).into())
}

/// Streams the turn's `AgentEvent`s over `on_event` and returns once it ends.
/// Deliberately awaits the whole stream rather than returning early: a
/// mid-stream failure comes back as this call's `Err`, since `AgentEvent`
/// has no error variant and should not grow one for a transport concern.
#[tauri::command]
pub async fn send_message(
    service: tauri::State<'_, Service>,
    conversation_id: ConversationId,
    text: String,
    on_event: tauri::ipc::Channel<AgentEvent>,
) -> Result<TurnOutcome, ErrorReport> {
    service
        .send_message(conversation_id, text, &ChannelSink(on_event))
        .await
        .map_err(|err| (&err).into())
}

#[tauri::command]
pub async fn approve_tools(
    service: tauri::State<'_, Service>,
    conversation_id: ConversationId,
    decisions: Vec<ToolDecision>,
    on_event: tauri::ipc::Channel<AgentEvent>,
) -> Result<TurnOutcome, ErrorReport> {
    service
        .approve_tools(conversation_id, decisions, &ChannelSink(on_event))
        .await
        .map_err(|err| (&err).into())
}

#[tauri::command]
pub async fn cancel_pending(
    service: tauri::State<'_, Service>,
    conversation_id: ConversationId,
) -> Result<(), ErrorReport> {
    service.cancel_pending(conversation_id).await.map_err(|err| (&err).into())
}
