use std::path::PathBuf;

/// Errors from the DB layer: opening the database file, running migrations,
/// executing queries against the DAL, and vector-dimension validation.
///
/// Variants are deliberately narrow so `to_json_line` / `exit_code` in
/// [`crate::error::CliError`] can discriminate for script callers.
#[derive(thiserror::Error, Debug)]
pub enum DbError {
    #[error("opening database at {path}: {source}")]
    Open { path: PathBuf, source: turso::Error },

    #[error("applying migration {version} ({name}): {source}")]
    Migrate {
        version: u32,
        name: &'static str,
        source: turso::Error,
    },

    #[error("running query ({op}): {source}")]
    Query {
        op: &'static str,
        source: turso::Error,
    },

    #[error("embedding dimension mismatch: expected {expected}, got {actual}")]
    DimensionMismatch { expected: usize, actual: usize },

    #[error("{kind} with id {id} not found")]
    NotFound { kind: &'static str, id: i64 },
}
