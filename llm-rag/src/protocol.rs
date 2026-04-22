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
    DocumentList {
        tags: Vec<String>,
        limit: Option<usize>,
    },
    DocumentDelete {
        id: i64,
    },
    /// Streaming ingest: the server reads `path` from its own filesystem,
    /// chunks it, embeds each chunk, and persists the results transactionally.
    /// Emits `DocumentCreateStart`, zero-or-more `DocumentCreateProgress`, and
    /// a terminal `DocumentCreateDone` (or `Error`) frame.
    DocumentCreate {
        path: String,
        tags: Vec<String>,
    },
    DocumentAddTag {
        id: i64,
        tag: String,
    },
    DocumentRemoveTag {
        id: i64,
        tag: String,
    },
    DocumentTags {
        id: i64,
    },
    DocumentSearch {
        query: String,
        tags: Vec<String>,
        limit: Option<usize>,
    },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum Response {
    Pong,
    /// First frame of a streaming chat response. Confirms/mints the
    /// conversation id. Omitted when the chat fails before LLM setup; in that
    /// case the very first frame is `Response::Error`.
    ChatStart {
        conversation_id: String,
    },
    /// Incremental assistant text. Zero or more between `ChatStart` and
    /// `ChatDone`.
    ChatDelta {
        conversation_id: String,
        text: String,
    },
    /// Incremental tool-call arguments. `name` is populated on the first
    /// fragment for a given `id` (may be None on subsequent fragments).
    ChatToolCallDelta {
        conversation_id: String,
        id: String,
        name: Option<String>,
        arguments_delta: String,
    },
    /// The server-side result of executing a tool call. Emitted after the
    /// corresponding tool_use row has been persisted and before the next
    /// `ChatDelta`/`ChatToolCallDelta` round starts. `id` matches the
    /// `ChatToolCallDelta` id.
    ChatToolResult {
        conversation_id: String,
        id: String,
        content: String,
    },
    /// Terminator for a streaming chat response. Carries the authoritative
    /// persisted rows (assistant text + tool_use rows — user row is omitted
    /// since the TUI echoed it optimistically).
    ChatDone {
        conversation_id: String,
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
    DocumentList {
        items: Vec<DocumentSummary>,
    },
    DocumentTags {
        tags: Vec<String>,
    },
    /// First frame of a streaming `DocumentCreate`. Confirms the new row id
    /// and (once chunking is done) the total number of chunks that will be
    /// embedded + persisted.
    DocumentCreateStart {
        document_id: i64,
        total_chunks: usize,
    },
    /// Progress update during a streaming ingest. Emitted at the start of
    /// each embed sub-batch (to announce chunks going in-flight) and after
    /// each chunk is persisted (to retire one from in-flight into completed).
    /// `completed + in_flight <= total`; `total == completed` when the
    /// terminal `DocumentCreateDone` is about to fire.
    DocumentCreateProgress {
        document_id: i64,
        completed: usize,
        in_flight: usize,
        total: usize,
    },
    /// Terminator for a successful ingest.
    DocumentCreateDone {
        document_id: i64,
        chunks: usize,
    },
    DocumentSearch {
        results: Vec<DocumentSearchHit>,
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DocumentSummary {
    pub id: i64,
    pub path: String,
    pub created_at: String,
    pub tags: Vec<String>,
}

/// One result row from a `DocumentSearch`. `range_kind` / `range_start` /
/// `range_end` flatten the DAL's `ChunkRange` enum for a simpler on-wire type.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DocumentSearchHit {
    pub document_id: i64,
    pub path: String,
    pub chunk_id: i64,
    pub content: String,
    pub distance: f32,
    pub range_kind: String,
    pub range_start: i64,
    pub range_end: i64,
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
