//! `AgentConfig`: the saved, user-editable definition of an agent — what CRUD
//! operates on, and what a conversation is started from.
//!
//! Distinct from [`crate::agent::AgentSpec`]: that's the *resolved*
//! definition of a running agent, rebuilt from a config's tool selection by
//! `lib::agent::registry::build_agent` every time an `Agent` is constructed,
//! and deliberately carries no identity or timestamps of its own — see its
//! own module doc. One `AgentConfig` builds one `AgentSpec`, never the
//! reverse.

use serde::{Deserialize, Serialize};

use crate::ids::AgentConfigId;
use crate::llm::model::ModelRef;
use crate::llm::tool::{Thinking, ToolChoice};

/// A saved agent definition, as CRUD returns it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentConfig {
    pub id: AgentConfigId,
    #[serde(flatten)]
    pub input: AgentConfigInput,
    /// ISO8601 UTC, millisecond precision — the same shape the log files and
    /// `ClientLogRecord::timestamp` use.
    pub created_at: String,
    pub updated_at: String,
}

/// The editable fields of an [`AgentConfig`] — what `create`/`update` take.
/// Mirrors `shared::agent::AgentSpec` field-for-field (model, system, tools,
/// tool_choice, thinking, stop_sequences, max_tokens, max_steps) plus a name
/// and an optional description, since `AgentSpec` itself has neither.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentConfigInput {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub model: ModelRef,
    /// One or more strings, joined by [`crate::agent::AgentSpec::system_prompt`]
    /// once this config is resolved into a running agent.
    #[serde(default)]
    pub system: Vec<String>,
    /// A token budget for **each** model call — see `AgentSpec::max_tokens`.
    pub max_tokens: u32,
    /// Names registered with `lib::agent::registry::ToolRegistry` — the
    /// process-wide catalog of tools this build knows how to execute.
    /// Approval is entirely the tool's own (`lib::agent::Tool::approval`);
    /// a config can select a tool or not, but never re-gate it.
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    #[serde(default)]
    pub thinking: Thinking,
    #[serde(default)]
    pub stop_sequences: Vec<String>,
    /// Cap on model calls in one turn — see `AgentSpec::max_steps`.
    pub max_steps: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> AgentConfigInput {
        AgentConfigInput {
            name: "ops-assistant".to_string(),
            description: Some("Runs deploys".to_string()),
            model: ModelRef::new("anthropic", "claude-opus-5"),
            system: vec!["You are a helpful ops assistant.".to_string()],
            max_tokens: 1024,
            tools: vec!["deploy".to_string()],
            tool_choice: None,
            thinking: Thinking::default(),
            stop_sequences: vec![],
            max_steps: 8,
        }
    }

    #[test]
    fn agent_config_round_trips_through_json_with_id_and_timestamps_flattened_in() {
        let config = AgentConfig {
            id: AgentConfigId(1),
            input: input(),
            created_at: "2026-08-22T00:00:00.000Z".to_string(),
            updated_at: "2026-08-22T00:00:00.000Z".to_string(),
        };
        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json["id"], serde_json::json!(1));
        assert_eq!(json["name"], serde_json::json!("ops-assistant"));
        assert_eq!(json["created_at"], serde_json::json!("2026-08-22T00:00:00.000Z"));
        let back: AgentConfig = serde_json::from_value(json).unwrap();
        assert_eq!(back, config);
    }

    #[test]
    fn tools_serialize_as_a_plain_array_of_names() {
        let json = serde_json::to_value(input()).unwrap();
        assert_eq!(json["tools"], serde_json::json!(["deploy"]));
    }

    #[test]
    fn agent_config_input_round_trips() {
        let original = input();
        let json = serde_json::to_string(&original).unwrap();
        let back: AgentConfigInput = serde_json::from_str(&json).unwrap();
        assert_eq!(back, original);
    }
}
