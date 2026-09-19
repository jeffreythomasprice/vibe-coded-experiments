use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("no output destination: stdout is not a terminal; pass -o <PATH> or --show")]
    NoOutputTarget,

    #[error("--output {} is a directory; pass a file path", .path.display())]
    OutputIsDirectory { path: PathBuf },

    #[error("expected {expected} generated images but found {found} in {}", .dir.display())]
    BatchOutputMismatch {
        expected: u32,
        found: usize,
        dir: PathBuf,
    },

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("image error: {0}")]
    Image(#[from] image::ImageError),

    #[error("failed to display image in terminal: {0}")]
    Display(#[from] viuer::ViuError),

    #[error("invalid diffusion model configuration: {0}")]
    Config(#[from] diffusion_rs::api::ConfigBuilderError),

    #[error("image generation failed: {0}")]
    Generation(#[from] diffusion_rs::api::DiffusionError),

    #[error(transparent)]
    ConfigFile(#[from] crate::config::ConfigError),

    #[error(transparent)]
    Log(#[from] crate::logging::LogError),

    #[error(transparent)]
    Model(#[from] crate::models::ModelError),

    #[error(
        "--backend {backend} was requested, but this binary was built without `--features {backend}`"
    )]
    UnsupportedBackend { backend: &'static str },
}
