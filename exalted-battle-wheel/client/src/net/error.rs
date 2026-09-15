#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RtcError {
    #[error("could not create the peer connection: {0}")]
    PeerConnection(String),
    #[error("could not create an {kind}: {message}")]
    Negotiate { kind: &'static str, message: String },
    #[error("could not set the {which} description: {message}")]
    Description { which: &'static str, message: String },
    #[error("the data channel is not open")]
    ChannelClosed,
    #[error("could not send on the data channel: {0}")]
    Send(String),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SignalError {
    #[error("that code is not valid base64")]
    NotBase64,
    #[error("that code is not valid text")]
    NotUtf8,
    #[error("that code is corrupted or was truncated")]
    Corrupted,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RoomError {
    #[error(transparent)]
    Rtc(#[from] RtcError),
    #[error(transparent)]
    Signal(#[from] SignalError),
}
