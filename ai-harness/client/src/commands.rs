//! One thin async function per Tauri command this UI calls.
//!
//! Centralizes the one bit of IPC plumbing every call site would otherwise
//! repeat: a Tauri command's JS-side args object is keyed by its *parameter
//! names* (camelCased), not by the shape of any one parameter — so
//! `send_message(conversation_id, text, on_event)` wants `{conversationId,
//! text, onEvent}`, not `conversation_id` merged in some other way. Each
//! function here owns exactly one such args shape.

use serde::Serialize;
use shared::agent::event::AgentEvent;
use shared::agent::{AgentConfig, AgentConfigInput, ToolSpec};
use shared::conversation::{ConversationSummary, ConversationView, ListConversations, TurnOutcome};
use shared::error::ErrorReport;
use shared::ids::{AgentConfigId, ConversationId};

use crate::ipc;

pub async fn list_agents() -> Result<Vec<AgentConfig>, ErrorReport> {
    ipc::call0("list_agents").await
}

pub async fn tool_catalog() -> Result<Vec<ToolSpec>, ErrorReport> {
    ipc::call0("tool_catalog").await
}

pub async fn create_agent(input: AgentConfigInput) -> Result<AgentConfig, ErrorReport> {
    #[derive(Serialize)]
    struct Args {
        input: AgentConfigInput,
    }
    ipc::call("create_agent", &Args { input }).await
}

pub async fn delete_agent(id: AgentConfigId) -> Result<(), ErrorReport> {
    #[derive(Serialize)]
    struct Args {
        id: AgentConfigId,
    }
    ipc::call("delete_agent", &Args { id }).await
}

pub async fn list_conversations(query: ListConversations) -> Result<Vec<ConversationSummary>, ErrorReport> {
    #[derive(Serialize)]
    struct Args {
        query: ListConversations,
    }
    ipc::call("list_conversations", &Args { query }).await
}

pub async fn create_conversation(
    agent_config_id: AgentConfigId,
    title: Option<String>,
) -> Result<ConversationSummary, ErrorReport> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Args {
        agent_config_id: AgentConfigId,
        title: Option<String>,
    }
    ipc::call("create_conversation", &Args { agent_config_id, title }).await
}

pub async fn get_conversation(id: ConversationId) -> Result<ConversationView, ErrorReport> {
    #[derive(Serialize)]
    struct Args {
        id: ConversationId,
    }
    ipc::call("get_conversation", &Args { id }).await
}

/// Sends `text` on `conversation_id`, streaming `AgentEvent`s to `on_event`
/// as they arrive, and resolving once the turn settles.
pub async fn send_message(
    conversation_id: ConversationId,
    text: String,
    on_event: impl FnMut(AgentEvent) + 'static,
) -> Result<TurnOutcome, ErrorReport> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Args {
        conversation_id: ConversationId,
        text: String,
    }
    ipc::call_streaming("send_message", &Args { conversation_id, text }, on_event).await
}

/// Read one row from the `preferences` table. `Ok(None)` means "not set" —
/// the caller decides what the default is.
pub async fn get_preference(key: &str) -> Result<Option<String>, ErrorReport> {
    #[derive(Serialize)]
    struct Args<'a> {
        key: &'a str,
    }
    ipc::call("get_preference", &Args { key }).await
}

/// Insert or replace one `preferences` row.
pub async fn set_preference(key: &str, value: &str) -> Result<(), ErrorReport> {
    #[derive(Serialize)]
    struct Args<'a> {
        key: &'a str,
        value: &'a str,
    }
    ipc::call("set_preference", &Args { key, value }).await
}
