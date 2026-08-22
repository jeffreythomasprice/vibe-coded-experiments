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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChatOptions {
    /// `None` means "use the provider's configured default model".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Anthropic's `max_tokens` (required by that API), OpenAI's
    /// `max_output_tokens`, Ollama's `options.num_predict`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub tools: Vec<ToolDef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    #[serde(default)]
    pub thinking: Thinking,
    #[serde(default)]
    pub stop_sequences: Vec<String>,
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
    fn chat_options_default_omits_every_optional_field() {
        let json = serde_json::to_value(ChatOptions::default()).unwrap();
        assert!(json.get("model").is_none());
        assert!(json.get("max_tokens").is_none());
        assert!(json.get("tool_choice").is_none());
        assert_eq!(json["tools"], serde_json::json!([]));
        assert_eq!(json["thinking"], serde_json::json!({"type": "off"}));
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
