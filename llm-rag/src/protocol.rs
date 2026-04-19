use futures::{SinkExt, StreamExt};
use serde::{Serialize, de::DeserializeOwned};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

use crate::error::ProtocolError;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum Request {
    Ping,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum Response {
    Pong,
    Error { message: String },
}

/// Wrap a stream with a u32 big-endian length prefix codec.
pub fn framed<S>(stream: S) -> Framed<S, LengthDelimitedCodec>
where
    S: AsyncRead + AsyncWrite,
{
    LengthDelimitedCodec::builder()
        .length_field_length(4)
        .max_frame_length(16 * 1024 * 1024)
        .new_framed(stream)
}

pub async fn write_frame<S, T>(
    fr: &mut Framed<S, LengthDelimitedCodec>,
    msg: &T,
) -> Result<(), ProtocolError>
where
    S: AsyncRead + AsyncWrite + Unpin,
    T: Serialize,
{
    let bytes = serde_json::to_vec(msg)?;
    fr.send(bytes.into()).await?;
    Ok(())
}

pub async fn read_frame<S, T>(fr: &mut Framed<S, LengthDelimitedCodec>) -> Result<T, ProtocolError>
where
    S: AsyncRead + AsyncWrite + Unpin,
    T: DeserializeOwned,
{
    let frame = match fr.next().await {
        Some(result) => result?,
        None => return Err(ProtocolError::StreamClosed),
    };
    Ok(serde_json::from_slice(&frame)?)
}
