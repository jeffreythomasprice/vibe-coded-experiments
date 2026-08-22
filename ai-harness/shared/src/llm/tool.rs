//! Tool definitions and per-request chat options.

use serde::{Deserialize, Serialize};

/// A tool the model may call. `input_schema` is a JSON Schema object,
/// deliberately untyped — see `ContentBlock::ToolUse::input`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Whether/which tool the model must use. Ollama accepts and silently
/// ignores this field — see `lib::llm::ollama`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolChoice {
    Auto,
    Any,
    None,
    Tool { name: String },
}

/// Per-request options shared across all three providers.
///
/// Deliberately has **no `temperature` field.** Adding one would be an
/// attractive nuisance: `temperature`/`top_p`/`top_k` are a hard 400 on
/// current Anthropic models (see `lib::llm::anthropic`), so a field that
/// silently breaks one of three providers is worse than no field at all.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatOptions {
    /// Which model to call. Required — a model id is a property of a
    /// request, not a deployment, so there is no configured default to fall
    /// back to. See `lib::llm::config`.
    pub model: String,
    /// A token budget the caller must choose. Anthropic's `max_tokens`
    /// (required by that API), OpenAI's `max_output_tokens`, Ollama's
    /// `options.num_predict`.
    pub max_tokens: u32,
    #[serde(default)]
    pub tools: Vec<ToolDef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    #[serde(default)]
    pub thinking: Thinking,
    #[serde(default)]
    pub stop_sequences: Vec<String>,
}

impl ChatOptions {
    /// Both a model and a token budget are per-request decisions with no
    /// sensible default — see the crate-level note in `lib::llm::config`.
    /// Every other field starts at its zero value:
    /// `ChatOptions { thinking: ..., ..ChatOptions::new(MODEL, 1024) }`.
    pub fn new(model: impl Into<String>, max_tokens: u32) -> Self {
        Self {
            model: model.into(),
            max_tokens,
            tools: Vec::new(),
            tool_choice: None,
            thinking: Thinking::default(),
            stop_sequences: Vec::new(),
        }
    }
}

/// Extended-thinking configuration, unified across providers' differing
/// vocabulary (Anthropic `thinking` + `output_config.effort`; OpenAI
/// reasoning effort; Ollama `think`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Thinking {
    #[default]
    Off,
    Adaptive {
        effort: Effort,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Effort {
    Low,
    Medium,
    High,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_options_new_omits_every_optional_field_but_model_and_max_tokens() {
        let json = serde_json::to_value(ChatOptions::new("claude-opus-5", 1024)).unwrap();
        assert_eq!(json["model"], serde_json::json!("claude-opus-5"));
        assert_eq!(json["max_tokens"], serde_json::json!(1024));
        assert!(json.get("tool_choice").is_none());
        assert_eq!(json["tools"], serde_json::json!([]));
        assert_eq!(json["thinking"], serde_json::json!({"type": "off"}));
    }

    #[test]
    fn chat_options_requires_model_and_max_tokens_on_deserialize() {
        let err = serde_json::from_value::<ChatOptions>(serde_json::json!({
            "max_tokens": 1024,
        }))
        .unwrap_err();
        assert!(err.to_string().contains("model"), "error was: {err}");

        let err = serde_json::from_value::<ChatOptions>(serde_json::json!({
            "model": "claude-opus-5",
        }))
        .unwrap_err();
        assert!(err.to_string().contains("max_tokens"), "error was: {err}");
    }

    #[test]
    fn tool_choice_tool_carries_a_name() {
        let choice = ToolChoice::Tool {
            name: "get_weather".to_string(),
        };
        let json = serde_json::to_value(&choice).unwrap();
        assert_eq!(json, serde_json::json!({"type": "tool", "name": "get_weather"}));
    }

    #[test]
    fn thinking_adaptive_round_trips_with_effort() {
        let thinking = Thinking::Adaptive { effort: Effort::High };
        let json = serde_json::to_string(&thinking).unwrap();
        let back: Thinking = serde_json::from_str(&json).unwrap();
        assert_eq!(back, thinking);
    }
}
