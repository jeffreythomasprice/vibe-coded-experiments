//! Turso-backed storage layer.
//!
//! This module owns:
//! - [`error::DbError`] — the error taxonomy for DB operations.
//! - [`schema::EmbeddingModelInfo`] — (name, dimensions) for the active
//!   embedding model; used to select the per-dimension chunk table.
//! - [`migrations`] — a tiny embedded migration runner that applies numbered
//!   `.sql` files recorded in `_schema_migrations`.
//! - [`dal::Dal`] — the server-side data access layer exposing document /
//!   tag / chunk CRUD and embedding-vector search.
//!
//! Public entry point: [`open`] — builds a fully-migrated [`Dal`] given the
//! app [`Config`] and the active embedding model.
#![allow(dead_code)]

pub mod dal;
pub mod error;
pub mod migrations;
pub mod schema;

pub use dal::{
    ChunkMatch, ChunkRange, Conversation, ConversationFilter, Dal, Document, DocumentFilter,
    MessageMetadata, MessageRole, StoredMessage,
};
pub use error::DbError;
pub use schema::{EmbeddingModelInfo, EmbeddingProbe};

use std::future::Future;

use crate::config::Config;

/// Open the configured DB file, run pending migrations, resolve the embedding
/// model's vector length and max input-token context length (cached or
/// probed), ensure the dimension-specific chunk table exists, and return a
/// ready-to-use [`Dal`].
///
/// `probe` is called at most once — only on a cache miss (or when the cached
/// row pre-dates `max_input_tokens` so the column is `NULL`) — to ask the
/// embedding provider for its metadata. Results are persisted so subsequent
/// boots are zero-cost.
pub async fn open<F, Fut>(cfg: &Config, model_name: String, probe: F) -> Result<Dal, DbError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<EmbeddingProbe, DbError>>,
{
    Dal::open(&cfg.db_path, model_name, probe).await
}
