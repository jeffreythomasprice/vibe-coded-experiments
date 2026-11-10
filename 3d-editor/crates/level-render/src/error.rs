use level_cmd::CommandError;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RenderError {
    #[error("no usable graphics adapter: {0}")]
    NoAdapter(String),
    #[error("could not create a graphics device: {0}")]
    Device(String),
    #[error("could not read back the image: {0}")]
    Readback(String),
    #[error("could not encode the image: {0}")]
    Encode(String),
}

impl From<RenderError> for CommandError {
    fn from(e: RenderError) -> Self {
        CommandError::Render {
            message: e.to_string(),
        }
    }
}
