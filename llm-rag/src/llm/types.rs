use serde_json::Value;

/// Conversation messages in our generic LLM interface. Each variant carries
/// exactly the data it needs; mapping to provider-specific message types is
/// handled in `llm::convert`.
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    System {
        content: String,
    },
    User {
        content: String,
    },
    Assistant {
        content: String,
        /// Empty when the assistant did not call any tools.
        tool_calls: Vec<ToolCall>,
    },
    Tool {
        tool_call_id: String,
        content: String,
    },
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self::System {
            content: content.into(),
        }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self::User {
            content: content.into(),
        }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self::Assistant {
            content: content.into(),
            tool_calls: Vec::new(),
        }
    }
    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self::Tool {
            tool_call_id: tool_call_id.into(),
            content: content.into(),
        }
    }
}

/// A tool definition offered to the model.
#[derive(Debug, Clone, PartialEq)]
pub struct Tool {
    pub name: String,
    pub description: String,
    /// JSON Schema describing the tool's parameters.
    pub parameters: Value,
}

/// A tool call emitted by the model.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, Default)]
pub struct ChatRequest {
    pub messages: Vec<Message>,
    pub tools: Vec<Tool>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u64>,
    /// Optional JSON Schema. When set, the provider is instructed (via system
    /// prompt) to respond with JSON conforming to the schema; the parsed
    /// result lands in `ChatResponse.structured`.
    pub json_schema: Option<Value>,
}

#[derive(Debug, Clone, Default)]
pub struct ChatResponse {
    /// Concatenated assistant text (empty when the model only called tools).
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    /// Populated when `ChatRequest.json_schema` was set and parsing succeeded.
    pub structured: Option<Value>,
}

/// One event in a streaming chat response. The stream terminates after `Done`.
#[derive(Debug, Clone)]
pub enum StreamChunk {
    Text(String),
    ToolCallDelta {
        id: String,
        name: Option<String>,
        arguments_delta: String,
    },
    Done,
}
