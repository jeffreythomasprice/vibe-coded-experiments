use futures::{SinkExt, StreamExt};
use serde::{Serialize, de::DeserializeOwned};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

use crate::db::{MessageMetadata, MessageRole};
use crate::error::ProtocolError;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum Request {
    Ping,
    Chat {
        conversation_id: Option<String>,
        message: String,
    },
    ConversationList {
        tags: Vec<String>,
        text_query: Option<String>,
        limit: Option<usize>,
    },
    ConversationGet {
        id: String,
    },
    ConversationDelete {
        id: String,
    },
    ConversationAddTag {
        id: String,
        tag: String,
    },
    ConversationRemoveTag {
        id: String,
        tag: String,
    },
    ConversationTags {
        id: String,
    },
    TagList,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum Response {
    Pong,
    Chat {
        conversation_id: String,
        reply: String,
        messages_appended: Vec<WireMessage>,
    },
    ConversationList {
        items: Vec<ConversationSummary>,
    },
    ConversationGet {
        conversation: ConversationSummary,
        messages: Vec<WireMessage>,
    },
    ConversationTags {
        tags: Vec<String>,
    },
    TagList {
        tags: Vec<String>,
    },
    Ok,
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConversationSummary {
    pub id: String,
    pub title: Option<String>,
    pub updated_at: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WireMessage {
    pub role: MessageRole,
    pub content: String,
    pub metadata: Option<MessageMetadata>,
    pub created_at: String,
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
