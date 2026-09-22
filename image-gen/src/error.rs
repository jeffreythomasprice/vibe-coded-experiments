use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("--output {} is a directory; pass a file path", .path.display())]
    OutputIsDirectory { path: PathBuf },

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("image error: {0}")]
    Image(#[from] image::ImageError),

    #[error("failed to display image in terminal: {0}")]
    Display(#[from] viuer::ViuError),

    #[error(transparent)]
    ConfigFile(#[from] crate::config::ConfigError),

    #[error(transparent)]
    Log(#[from] crate::logging::LogError),

    #[error(transparent)]
    Model(#[from] crate::models::ModelError),

    #[error(transparent)]
    Hub(#[from] crate::hub::HubError),

    #[error(transparent)]
    Llm(#[from] crate::llm::LlmError),

    #[error(transparent)]
    Eval(#[from] crate::eval::EvalError),

    #[error(transparent)]
    Sd(#[from] crate::sd::SdError),

    #[error("no checkpoint: pass --model or --diffusion-model, or a --preset that sets one")]
    NoCheckpoint,

    #[error("--ref-image {} does not exist", .path.display())]
    RefImageNotFound { path: PathBuf },

    #[error("unknown preset '{name}'; config defines: {available}")]
    UnknownPreset { name: String, available: String },
}
