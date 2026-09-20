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
}

/// A backend capable of running one turn of a chat, with or without tool calls.
/// Streaming is deliberately not part of this trait; it would be a separate method
/// added when something needs it, not a variant of `chat`.
#[async_trait]
pub trait Provider: Send + Sync {
    fn name(&self) -> &'static str;
    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, LlmError>;
}
