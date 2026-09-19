use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("no output destination: stdout is not a terminal; pass -o <PATH> or --show")]
    NoOutputTarget,

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
    Model(#[from] crate::models::ModelError),
}
