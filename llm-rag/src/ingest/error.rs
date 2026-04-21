use std::path::PathBuf;

use crate::db::DbError;
use crate::llm::LlmError;

#[derive(thiserror::Error, Debug)]
pub enum IngestError {
    #[error("reading {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("{path} is {actual} bytes, over the {max}-byte ingest cap")]
    TooLarge {
        path: PathBuf,
        actual: u64,
        max: u64,
    },

    #[error("{path} does not look like a text file: {reason}")]
    NotTextFile { path: PathBuf, reason: &'static str },

    #[error("embedding failed: {0}")]
    Embed(#[from] LlmError),

    #[error("persisting document: {0}")]
    Persist(#[from] DbError),
}
