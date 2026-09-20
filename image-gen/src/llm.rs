pub mod agent;
pub mod config;
pub mod message;
pub mod ollama;
pub mod provider;
pub mod runtime;
pub mod tool;

#[cfg(test)]
pub(crate) mod mock;

use std::path::PathBuf;

use thiserror::Error;

pub use agent::{Agent, AgentOutcome};
pub use config::LlmConfig;
pub use message::{Content, Image, MediaType, Message, Role};
pub use provider::{
    ChatOptions, ChatRequest, ChatResponse, Provider, StopReason, TokenChoice, TokenLogprob, Usage,
};
pub use runtime::Llm;
pub use tool::{FunctionTool, Tool, ToolCall, ToolDef, ToolFailure, ToolOutput, ToolRegistry};

#[derive(Debug, Error)]
pub enum LlmError {
    #[error("request to {url} failed: {source}")]
    Request {
        url: String,
        #[source]
        source: Box<reqwest::Error>,
    },

    #[error("{provider} returned {status} for {url}: {body}")]
    Status {
        provider: &'static str,
        status: u16,
        url: String,
        body: String,
    },

    #[error("failed to parse the {provider} response from {url}: {source}")]
    Decode {
        provider: &'static str,
        url: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("{provider} returned an empty response")]
    EmptyResponse { provider: &'static str },

    #[error(
        "model '{model}' was pulled successfully after {original}, but the chat request failed again: {retry}"
    )]
    PullRetryFailed {
        model: String,
        original: Box<LlmError>,
        #[source]
        retry: Box<LlmError>,
    },

    #[error("pulling model '{model}' failed: {message}")]
    PullFailed { model: String, message: String },

    #[error("the pull of model '{model}' ended without reporting success")]
    PullIncomplete { model: String },

    #[error("a tool named '{name}' is already registered")]
    DuplicateTool { name: String },

    #[error("unknown tool '{name}'; registered tools: {available}")]
    UnknownTool { name: String, available: String },

    #[error("tool '{tool}' rejected its arguments: {message}")]
    ToolArguments { tool: String, message: String },

    #[error("tool '{tool}' failed: {message}")]
    ToolFailed { tool: String, message: String },

    #[error("agent exceeded its limit of {limit} turns without a final response")]
    MaxTurns { limit: usize },

    #[error("failed to read image {}: {source}", .path.display())]
    ImageRead {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("unrecognized image type{}", .path.as_ref().map(|p| format!(" for {}", p.display())).unwrap_or_default())]
    UnsupportedImageType { path: Option<PathBuf> },

    #[error("no model for --{flag}: pass it explicitly, set [llm].model in config.toml, or use a preset that sets one")]
    MissingModel { flag: &'static str },
}
