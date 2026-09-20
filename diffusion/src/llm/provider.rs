use async_trait::async_trait;

use crate::llm::LlmError;
use crate::llm::message::Message;
use crate::llm::tool::ToolDef;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChatOptions {
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub seed: Option<i64>,
    pub max_tokens: Option<u32>,
    pub stop: Vec<String>,
    /// Request this many top alternative tokens (with probabilities) per generated
    /// token, for scorers that read logprobs directly rather than the generated
    /// text (VQAScore).
    pub logprobs: Option<u8>,
    /// Explicitly enable/disable a reasoning model's thinking step. `Some(false)`
    /// is required for logprob-based scoring: without it a thinking model emits
    /// reasoning tokens first and the logprobs describe those, not an answer.
    pub think: Option<bool>,
    /// A JSON Schema the reply must conform to, for structured output.
    pub format: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TokenLogprob {
    pub token: String,
    pub logprob: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TokenChoice {
    pub token: String,
    pub logprob: f32,
    pub top: Vec<TokenLogprob>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDef>,
    pub options: ChatOptions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    Stop,
    Length,
    ToolCalls,
    Other(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatResponse {
    pub message: Message,
    pub stop_reason: StopReason,
    pub usage: Usage,
    /// Per-token top alternatives, populated only when `ChatOptions::logprobs`
    /// was set; empty otherwise, matching the empty-means-absent convention used
    /// elsewhere in this module (`tool_calls`, `tools`, `options.stop`).
    pub logprobs: Vec<TokenChoice>,
}

/// A backend capable of running one turn of a chat, with or without tool calls.
/// Streaming is deliberately not part of this trait; it would be a separate method
/// added when something needs it, not a variant of `chat`.
#[async_trait]
pub trait Provider: Send + Sync {
    fn name(&self) -> &'static str;
    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, LlmError>;
}
