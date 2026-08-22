//! Wire types for LLM conversations: messages, content blocks, tool
//! definitions and results, images, and streaming events.
//!
//! These types cross the Tauri IPC boundary into the Leptos frontend, so the
//! crate-level constraint in `shared::lib` applies: no `std::fs`, threads, or
//! the clock. `lib::llm` builds provider-specific requests from these types
//! and parses provider responses back into them.

pub mod embedding;
pub mod image;
pub mod message;
pub mod model;
pub mod stream;
pub mod tool;

pub use embedding::{Embedding, EmbeddingRequest, EmbeddingResponse, EmbeddingUsage};
pub use image::{GeneratedImage, ImageRequest, ImageSource, MediaType};
pub use message::{
    CompletedMessage, ContentBlock, Conversation, Message, Role, StopReason, ToolResultContent,
    Usage,
};
pub use model::{
    estimated_tokens, CatalogEntry, ModelCatalog, ModelDetails, ModelInfo, ModelRef,
    ProviderModels,
};
pub use stream::{Delta, StreamEvent};
pub use tool::{ChatOptions, Effort, Thinking, ToolChoice, ToolDef};
