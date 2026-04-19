use thiserror::Error;

#[derive(Debug, Error)]
pub enum LlmError {
    #[error("missing secret: {0}")]
    MissingSecret(&'static str),

    #[error("invalid llm config: {0}")]
    ConfigInvalid(String),

    #[error("provider error: {0}")]
    Provider(#[source] anyhow::Error),

    #[error("structured output parse failed: {source}; raw={raw}")]
    StructuredParse {
        #[source]
        source: serde_json::Error,
        raw: String,
    },

    #[error("stream error: {0}")]
    Stream(String),

    #[error("operation not supported: {0}")]
    Unsupported(&'static str),
}
