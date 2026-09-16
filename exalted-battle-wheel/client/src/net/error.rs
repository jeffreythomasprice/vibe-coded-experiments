#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SocketError {
    #[error("could not open a connection: {0}")]
    Connect(String),
    #[error("could not encode a message: {0}")]
    Encode(String),
    #[error("could not send a message: {0}")]
    Send(String),
}
