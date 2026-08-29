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
use shared::agent::{AgentConfig, AgentConfigInput, ToolDecision, ToolSpec};
use shared::conversation::{ConversationSummary, ConversationView, ListConversations, RunStatus};
use shared::error::ErrorReport;
use shared::ids::{AgentConfigId, ConversationId, ProjectId, ThemeId};
use shared::project::{Project, ProjectInput, ProjectRef, ProjectSnapshot, SandboxStatus};
use shared::theme::{UserTheme, UserThemeInput};

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
    project: ProjectRef,
    title: Option<String>,
) -> Result<ConversationSummary, ErrorReport> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Args {
        agent_config_id: AgentConfigId,
        project: ProjectRef,
        title: Option<String>,
    }
    ipc::call("create_conversation", &Args { agent_config_id, project, title }).await
}

pub async fn get_conversation(id: ConversationId) -> Result<ConversationView, ErrorReport> {
    #[derive(Serialize)]
    struct Args {
        id: ConversationId,
    }
    ipc::call("get_conversation", &Args { id }).await
}

/// Reserves `conversation_id`'s run and drives the turn on the backend,
/// detached from this call — it returns as soon as the turn is accepted, not
/// once it finishes. Watch it happen with [`attach_conversation`].
pub async fn start_message(conversation_id: ConversationId, text: String) -> Result<(), ErrorReport> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Args {
        conversation_id: ConversationId,
        text: String,
    }
    ipc::call("start_message", &Args { conversation_id, text }).await
}

/// The `approve_tools` counterpart to [`start_message`] — resolves every
/// pending call in `conversation_id`'s suspended turn and drives it forward,
/// detached the same way. Watch it happen with [`attach_conversation`].
pub async fn start_approve_tools(
    conversation_id: ConversationId,
    decisions: Vec<ToolDecision>,
) -> Result<(), ErrorReport> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Args {
        conversation_id: ConversationId,
        decisions: Vec<ToolDecision>,
    }
    ipc::call("start_approve_tools", &Args { conversation_id, decisions }).await
}

/// Discards `conversation_id`'s suspended turn without resolving it. The
/// conversation is left exactly as it was before the turn started.
pub async fn cancel_pending(conversation_id: ConversationId) -> Result<(), ErrorReport> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Args {
        conversation_id: ConversationId,
    }
    ipc::call("cancel_pending", &Args { conversation_id }).await
}

/// Replays whatever `conversation_id`'s run has already emitted to
/// `on_event`, then streams live events until it settles — the same
/// `AgentEvent`s a direct `send_message` used to stream, so
/// `transcript::Draft::apply` folds them identically either way.
pub async fn attach_conversation(
    conversation_id: ConversationId,
    on_event: impl FnMut(AgentEvent) + 'static,
) -> Result<RunStatus, ErrorReport> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Args {
        conversation_id: ConversationId,
    }
    ipc::call_streaming("attach_conversation", &Args { conversation_id }, on_event).await
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

pub async fn list_themes() -> Result<Vec<UserTheme>, ErrorReport> {
    ipc::call0("list_themes").await
}

pub async fn create_theme(input: UserThemeInput) -> Result<UserTheme, ErrorReport> {
    #[derive(Serialize)]
    struct Args {
        input: UserThemeInput,
    }
    ipc::call("create_theme", &Args { input }).await
}

pub async fn update_theme(id: ThemeId, input: UserThemeInput) -> Result<UserTheme, ErrorReport> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Args {
        id: ThemeId,
        input: UserThemeInput,
    }
    ipc::call("update_theme", &Args { id, input }).await
}

pub async fn delete_theme(id: ThemeId) -> Result<(), ErrorReport> {
    #[derive(Serialize)]
    struct Args {
        id: ThemeId,
    }
    ipc::call("delete_theme", &Args { id }).await
}

pub async fn list_projects() -> Result<Vec<Project>, ErrorReport> {
    ipc::call0("list_projects").await
}

/// Every project in "start a new conversation" order: the default project
/// first, then user projects by name.
pub async fn list_projects_for_picker() -> Result<Vec<ProjectSnapshot>, ErrorReport> {
    ipc::call0("list_projects_for_picker").await
}

pub async fn create_project(input: ProjectInput) -> Result<Project, ErrorReport> {
    #[derive(Serialize)]
    struct Args {
        input: ProjectInput,
    }
    ipc::call("create_project", &Args { input }).await
}

pub async fn update_project(id: ProjectId, input: ProjectInput) -> Result<Project, ErrorReport> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Args {
        id: ProjectId,
        input: ProjectInput,
    }
    ipc::call("update_project", &Args { id, input }).await
}

pub async fn delete_project(id: ProjectId) -> Result<(), ErrorReport> {
    #[derive(Serialize)]
    struct Args {
        id: ProjectId,
    }
    ipc::call("delete_project", &Args { id }).await
}

pub async fn sandbox_status() -> Result<SandboxStatus, ErrorReport> {
    ipc::call0("sandbox_status").await
}
