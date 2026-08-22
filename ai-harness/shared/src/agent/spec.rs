//! `AgentSpec`: the serializable *definition* of an agent — a model, a system
//! prompt, and a set of tools. Nothing executable lives here; `lib::agent::Agent`
//! is what actually runs one. Keeping the split lets the spec cross the Tauri
//! IPC boundary into the Leptos frontend for display or persistence without
//! that side ever needing `lib`'s async/tool-execution dependencies.

use serde::{Deserialize, Serialize};

use crate::llm::model::ModelRef;
use crate::llm::tool::{ChatOptions, Thinking, ToolChoice, ToolDef};

/// The definition of an agent: an exact model, a system prompt expressed as
/// one or more strings, and a set of tools.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSpec {
    pub model: ModelRef,
    /// One or more strings, joined by [`AgentSpec::system_prompt`].
    /// `shared::llm::Conversation::system` is a single `Option<String>` —
    /// joining here keeps that shape and every provider's `wire` module
    /// untouched, while still letting a caller compose a prompt from several
    /// pieces (a base persona, a task-specific addendum, injected context)
    /// without hand-managing the blank-line join themselves.
    #[serde(default)]
    pub system: Vec<String>,
    /// A token budget for **each** model call this agent makes, not for a
    /// whole multi-step turn — the same per-request field
    /// `shared::llm::ChatOptions` already requires. `max_steps` below is the
    /// control for a turn's total cost/runaway risk, not this.
    pub max_tokens: u32,
    #[serde(default)]
    pub tools: Vec<ToolSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    #[serde(default)]
    pub thinking: Thinking,
    #[serde(default)]
    pub stop_sequences: Vec<String>,
    /// Cap on model calls in one turn. Not a config default, for the same
    /// reason a model id isn't one — see `lib::llm::config`'s module doc: a
    /// per-agent decision, not a deployment setting.
    pub max_steps: u32,
}

impl AgentSpec {
    /// Both a model and a token budget are required, with no sensible
    /// default — mirrors `ChatOptions::new`. Every other field starts at its
    /// zero value.
    pub fn new(model: ModelRef, max_tokens: u32) -> Self {
        Self {
            model,
            system: Vec::new(),
            max_tokens,
            tools: Vec::new(),
            tool_choice: None,
            thinking: Thinking::default(),
            stop_sequences: Vec::new(),
            max_steps: default_max_steps(),
        }
    }

    /// Join `system` into the single string a `Conversation` carries, with a
    /// blank line between pieces. `None` when there is nothing to say —
    /// `Conversation::system` is itself an `Option`, and an empty string is
    /// not the same thing as "no system prompt" to a provider.
    pub fn system_prompt(&self) -> Option<String> {
        if self.system.is_empty() {
            return None;
        }
        Some(self.system.join("\n\n"))
    }

    /// Build the `ChatOptions` for one model call from this spec. `tools`
    /// carries every tool's declaration; approval routing is decided
    /// separately, by `lib::agent`'s loop, from each `ToolSpec::approval`.
    pub fn chat_options(&self) -> ChatOptions {
        ChatOptions {
            model: self.model.model.clone(),
            max_tokens: self.max_tokens,
            tools: self.tools.iter().map(|t| t.def.clone()).collect(),
            tool_choice: self.tool_choice.clone(),
            thinking: self.thinking.clone(),
            stop_sequences: self.stop_sequences.clone(),
        }
    }
}

fn default_max_steps() -> u32 {
    8
}

/// One tool an agent may call, plus whether calling it needs the user's
/// sign-off first.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub def: ToolDef,
    pub approval: Approval,
}

/// Whether a tool call may run unattended, or must be approved by the user
/// first. See `shared::agent::turn::TurnStop::AwaitingApproval`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Approval {
    #[default]
    Automatic,
    RequiresApproval,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> ModelRef {
        ModelRef::new("anthropic", "claude-opus-5")
    }

    #[test]
    fn new_sets_required_fields_and_sensible_zero_defaults() {
        let spec = AgentSpec::new(model(), 1024);
        assert_eq!(spec.model, model());
        assert_eq!(spec.max_tokens, 1024);
        assert!(spec.system.is_empty());
        assert!(spec.tools.is_empty());
        assert!(spec.tool_choice.is_none());
        assert_eq!(spec.thinking, Thinking::Off);
        assert!(spec.stop_sequences.is_empty());
        assert_eq!(spec.max_steps, 8);
    }

    #[test]
    fn system_prompt_is_none_when_there_is_nothing_to_say() {
        let spec = AgentSpec::new(model(), 1024);
        assert_eq!(spec.system_prompt(), None);
    }

    #[test]
    fn system_prompt_joins_multiple_strings_with_a_blank_line() {
        let mut spec = AgentSpec::new(model(), 1024);
        spec.system = vec![
            "You are a helpful assistant.".to_string(),
            "Be concise.".to_string(),
        ];
        assert_eq!(
            spec.system_prompt(),
            Some("You are a helpful assistant.\n\nBe concise.".to_string())
        );
    }

    #[test]
    fn chat_options_carries_every_field_off_the_spec() {
        let mut spec = AgentSpec::new(model(), 2048);
        spec.tools = vec![ToolSpec {
            def: ToolDef {
                name: "get_weather".to_string(),
                description: "Look up the weather".to_string(),
                input_schema: serde_json::json!({"type": "object"}),
            },
            approval: Approval::Automatic,
        }];
        spec.tool_choice = Some(ToolChoice::Auto);
        spec.thinking = Thinking::Adaptive {
            effort: crate::llm::tool::Effort::Low,
        };
        spec.stop_sequences = vec!["STOP".to_string()];

        let options = spec.chat_options();
        assert_eq!(options.model, "claude-opus-5");
        assert_eq!(options.max_tokens, 2048);
        assert_eq!(options.tools.len(), 1);
        assert_eq!(options.tools[0].name, "get_weather");
        assert!(matches!(options.tool_choice, Some(ToolChoice::Auto)));
        assert_eq!(options.thinking, spec.thinking);
        assert_eq!(options.stop_sequences, vec!["STOP".to_string()]);
    }

    #[test]
    fn agent_spec_round_trips_through_json() {
        let mut spec = AgentSpec::new(model(), 1024);
        spec.system = vec!["persona".to_string()];
        spec.tools.push(ToolSpec {
            def: ToolDef {
                name: "ping".to_string(),
                description: "ping".to_string(),
                input_schema: serde_json::json!({"type": "object"}),
            },
            approval: Approval::RequiresApproval,
        });
        let json = serde_json::to_string(&spec).unwrap();
        let back: AgentSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back.model, spec.model);
        assert_eq!(back.system, spec.system);
        assert_eq!(back.tools.len(), 1);
        assert_eq!(back.tools[0].approval, Approval::RequiresApproval);
    }

    #[test]
    fn approval_defaults_to_automatic() {
        assert_eq!(Approval::default(), Approval::Automatic);
    }
}
