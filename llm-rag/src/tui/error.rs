use crate::error::ClientError;

#[derive(thiserror::Error, Debug)]
pub enum TuiError {
    #[error("terminal io: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Client(#[from] ClientError),
}
