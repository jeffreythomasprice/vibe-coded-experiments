use std::path::PathBuf;

use futures::StreamExt;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::SendError;

use crate::db::{
    ChunkRange, ConversationFilter, DbError, DocumentFilter, MessageMetadata, MessageRole,
    StoredMessage,
};
use crate::ingest::{self, ChunkProgress, IngestError};
use crate::llm::types::{ChatRequest, Message, StreamChunk, ToolCall};
use crate::protocol::{
    ConversationSummary, DocumentSearchHit, DocumentSummary, Request, Response, WireMessage,
};
use crate::server::ServerState;

/// Dispatch a request, emitting one or more `Response` frames on `tx`. Most
/// handlers produce exactly one frame; chat produces a sequence (`ChatStart`,
/// zero-or-more `ChatDelta`/`ChatToolCallDelta`, terminated by `ChatDone` or
/// `Response::Error`).
///
/// Returns `Err(SendError)` only when the consumer of `tx` has dropped — i.e.
/// the connection's writer task died (usually client disconnect). The server
/// treats that as a signal to stop reading further requests on this connection.
pub async fn dispatch(
    req: Request,
    state: &ServerState,
    tx: &mpsc::Sender<Response>,
) -> Result<(), SendError<Response>> {
    match req {
        Request::Ping => tx.send(Response::Pong).await,
        Request::Chat {
            conversation_id,
            message,
        } => handle_chat(state, conversation_id, message, tx).await,
        Request::ConversationList {
            tags,
            text_query,
            limit,
        } => {
            let resp = match handle_conversation_list(state, tags, text_query, limit).await {
                Ok(r) => r,
                Err(err) => err_response(err),
            };
            tx.send(resp).await
        }
        Request::ConversationGet { id } => {
            let resp = match handle_conversation_get(state, id).await {
                Ok(r) => r,
                Err(err) => err_response(err),
            };
            tx.send(resp).await
        }
        Request::ConversationDelete { id } => {
            let resp = match state.dal.delete_conversation(&id).await {
                Ok(()) => Response::Ok,
                Err(err) => err_response(err),
            };
            tx.send(resp).await
        }
        Request::ConversationAddTag { id, tag } => {
            let resp = match state.dal.add_conversation_tag(&id, &tag).await {
                Ok(()) => Response::Ok,
                Err(err) => err_response(err),
            };
            tx.send(resp).await
        }
        Request::ConversationRemoveTag { id, tag } => {
            let resp = match state.dal.remove_conversation_tag(&id, &tag).await {
                Ok(()) => Response::Ok,
                Err(err) => err_response(err),
            };
            tx.send(resp).await
        }
        Request::ConversationTags { id } => {
            let resp = match state.dal.tags_for_conversation(&id).await {
                Ok(tags) => Response::ConversationTags { tags },
                Err(err) => err_response(err),
            };
            tx.send(resp).await
        }
        Request::TagList => {
            let resp = match state.dal.list_all_tags().await {
                Ok(tags) => Response::TagList { tags },
                Err(err) => err_response(err),
            };
            tx.send(resp).await
        }
        Request::DocumentList { tags, limit } => {
            let resp = match handle_document_list(state, tags, limit).await {
                Ok(r) => r,
                Err(err) => err_response(err),
            };
            tx.send(resp).await
        }
        Request::DocumentDelete { id } => {
            let resp = match state.dal.delete_document(id).await {
                Ok(()) => Response::Ok,
                Err(err) => err_response(err),
            };
            tx.send(resp).await
        }
        Request::DocumentCreate { path, tags } => {
            handle_document_create(state, path, tags, tx).await
        }
        Request::DocumentAddTag { id, tag } => {
            let resp = match state.dal.add_tag(id, &tag).await {
                Ok(()) => Response::Ok,
                Err(err) => err_response(err),
            };
            tx.send(resp).await
        }
        Request::DocumentRemoveTag { id, tag } => {
            let resp = match state.dal.remove_tag(id, &tag).await {
                Ok(()) => Response::Ok,
                Err(err) => err_response(err),
            };
            tx.send(resp).await
        }
        Request::DocumentTags { id } => {
            let resp = match state.dal.tags_for_document(id).await {
                Ok(tags) => Response::DocumentTags { tags },
                Err(err) => err_response(err),
            };
            tx.send(resp).await
        }
        Request::DocumentSearch { query, tags, limit } => {
            let resp = match handle_document_search(state, query, tags, limit).await {
                Ok(r) => r,
                Err(err) => Response::Error {
                    message: err.to_string(),
                },
            };
            tx.send(resp).await
        }
    }
}

/// Maximum number of LLM↔tool round trips for a single user message before we
/// bail. Picked defensively — real conversations rarely exceed 2 or 3.
const MAX_TOOL_ITERATIONS: usize = 5;

/// Accumulator for a single tool call as its argument fragments arrive. The
/// provider emits `ToolCallDelta { id, name, arguments_delta }` events; we
/// aggregate all fragments with the same `id`, capturing the first non-None
/// `name` seen, and parse `args` as JSON once the stream terminates.
struct ToolCallAccum {
    id: String,
    name: Option<String>,
    args: String,
}

/// Stream the LLM's reply to `message` in the context of `conversation_id`'s
/// history. Emits `ChatStart`, then zero-or-more `ChatDelta` /
/// `ChatToolCallDelta`, and terminates with either `ChatDone` (success, rows
/// persisted) or `Response::Error` (DB/LLM failure; no partial persistence).
///
/// A `None` id mints a new conversation. The `tx` send errors only if the
/// connection's writer task has dropped — that aborts the in-flight stream
/// without persisting anything.
async fn handle_chat(
    state: &ServerState,
    conversation_id: Option<String>,
    message: String,
    tx: &mpsc::Sender<Response>,
) -> Result<(), SendError<Response>> {
    match handle_chat_inner(state, conversation_id, message, tx).await {
        Ok(()) => Ok(()),
        Err(ChatFlow::WriterGone(e)) => Err(e),
        Err(ChatFlow::Emit(msg)) => tx.send(Response::Error { message: msg }).await,
    }
}

async fn handle_chat_inner(
    state: &ServerState,
    conversation_id: Option<String>,
    message: String,
    tx: &mpsc::Sender<Response>,
) -> Result<(), ChatFlow> {
    let conv_id = match conversation_id {
        Some(id) => id,
        None => {
            let id = uuid::Uuid::now_v7().hyphenated().to_string();
            state.dal.create_conversation(&id, None).await?;
            id
        }
    };

    // Load prior history BEFORE appending the new user turn — otherwise we'd
    // double-count it in the prompt we build below.
    let history = state.dal.messages_for_conversation(&conv_id).await?;

    state
        .dal
        .append_message(&conv_id, MessageRole::User, &message, None)
        .await?;

    let mut messages = Vec::with_capacity(history.len() + 1);
    for m in &history {
        messages.push(stored_to_llm(m)?);
    }
    messages.push(Message::user(message.clone()));

    tx.send(Response::ChatStart {
        conversation_id: conv_id.clone(),
    })
    .await?;

    let tool_definitions = state.tools.definitions();
    let mut appended: Vec<WireMessage> = Vec::new();

    for iteration in 0..MAX_TOOL_ITERATIONS {
        let (assistant_text, tool_calls) =
            run_one_stream(state, &conv_id, &messages, &tool_definitions, tx).await?;

        // Persist assistant text first (if any), then each tool call as its
        // own `tool_use` row. Both shapes are added to `appended` for the
        // terminal ChatDone frame and to `messages` so the next loop
        // iteration sees them.
        if !assistant_text.is_empty() {
            state
                .dal
                .append_message(&conv_id, MessageRole::Assistant, &assistant_text, None)
                .await?;
            appended.push(WireMessage {
                role: MessageRole::Assistant,
                content: assistant_text.clone(),
                metadata: None,
                created_at: String::new(),
            });
            messages.push(Message::assistant(assistant_text));
        }

        // If the model didn't call any tools, we're done.
        if tool_calls.is_empty() {
            return send_done(tx, conv_id, appended).await;
        }

        let parsed_calls =
            persist_tool_use_rows(state, &conv_id, &tool_calls, &mut appended).await?;

        // Each tool_use row becomes its own Assistant message (matches the
        // shape `stored_to_llm` would produce on reload) so the next round of
        // chat_stream sees the same history we'd rebuild from the DB.
        for call in &parsed_calls {
            messages.push(Message::Assistant {
                content: String::new(),
                tool_calls: vec![call.clone()],
            });
        }

        // Execute each tool, persist its result row, stream the result frame.
        for call in &parsed_calls {
            let result_str = match state.tools.invoke(&call.name, call.arguments.clone()).await {
                Ok(s) => s,
                Err(err) => format!("error: {err}"),
            };
            let meta = MessageMetadata::Tool {
                tool_call_id: call.id.clone(),
            };
            state
                .dal
                .append_message(&conv_id, MessageRole::Tool, &result_str, Some(&meta))
                .await?;
            appended.push(WireMessage {
                role: MessageRole::Tool,
                content: result_str.clone(),
                metadata: Some(meta),
                created_at: String::new(),
            });
            messages.push(Message::Tool {
                tool_call_id: call.id.clone(),
                content: result_str.clone(),
            });
            tx.send(Response::ChatToolResult {
                conversation_id: conv_id.clone(),
                id: call.id.clone(),
                content: result_str,
            })
            .await?;
        }

        // Safety net for the final iteration: don't loop forever if the model
        // keeps asking for tools. The ChatDone frame here still carries every
        // row persisted so far; clients can treat repeated tail-tool rows as a
        // signal to surface the cap to the user.
        if iteration + 1 == MAX_TOOL_ITERATIONS {
            return Err(ChatFlow::Emit(format!(
                "tool-call loop exceeded {MAX_TOOL_ITERATIONS} iterations"
            )));
        }
    }

    // Unreachable — the loop always either returns on "no tool calls" or
    // errors on the last-iteration guard above.
    unreachable!()
}

/// Consume one LLM streaming turn: forward every `ChatDelta` /
/// `ChatToolCallDelta` to `tx` while accumulating the assistant text and any
/// tool calls. Returns when the provider emits `StreamChunk::Done`.
async fn run_one_stream(
    state: &ServerState,
    conv_id: &str,
    messages: &[Message],
    tools: &[crate::llm::types::Tool],
    tx: &mpsc::Sender<Response>,
) -> Result<(String, Vec<ToolCallAccum>), ChatFlow> {
    let mut stream = state
        .llm
        .chat
        .chat_stream(ChatRequest {
            messages: messages.to_vec(),
            tools: tools.to_vec(),
            ..Default::default()
        })
        .await?;

    let mut assistant_text = String::new();
    let mut tool_calls: Vec<ToolCallAccum> = Vec::new();

    while let Some(chunk) = stream.next().await {
        match chunk? {
            StreamChunk::Text(t) => {
                assistant_text.push_str(&t);
                tx.send(Response::ChatDelta {
                    conversation_id: conv_id.to_string(),
                    text: t,
                })
                .await?;
            }
            StreamChunk::ToolCallDelta {
                id,
                name,
                arguments_delta,
            } => {
                match tool_calls.iter_mut().find(|tc| tc.id == id) {
                    Some(existing) => {
                        if existing.name.is_none() {
                            existing.name = name.clone();
                        }
                        existing.args.push_str(&arguments_delta);
                    }
                    None => tool_calls.push(ToolCallAccum {
                        id: id.clone(),
                        name: name.clone(),
                        args: arguments_delta.clone(),
                    }),
                }
                tx.send(Response::ChatToolCallDelta {
                    conversation_id: conv_id.to_string(),
                    id,
                    name,
                    arguments_delta,
                })
                .await?;
            }
            StreamChunk::Done => break,
        }
    }
    Ok((assistant_text, tool_calls))
}

/// Insert one `tool_use` DB row per accumulated call (+ the matching
/// `WireMessage` on `appended`), and return the parsed [`ToolCall`]s so the
/// executor can dispatch them by name.
async fn persist_tool_use_rows(
    state: &ServerState,
    conv_id: &str,
    tool_calls: &[ToolCallAccum],
    appended: &mut Vec<WireMessage>,
) -> Result<Vec<ToolCall>, ChatFlow> {
    let mut parsed: Vec<ToolCall> = Vec::with_capacity(tool_calls.len());
    for tc in tool_calls {
        // Fall back to storing the raw accumulated string if the model emitted
        // malformed JSON, rather than blowing up the whole turn.
        let arguments = serde_json::from_str::<serde_json::Value>(&tc.args)
            .unwrap_or_else(|_| serde_json::Value::String(tc.args.clone()));
        let name = tc.name.clone().unwrap_or_default();
        let meta = MessageMetadata::ToolUse {
            tool_call_id: tc.id.clone(),
            name: name.clone(),
            arguments: arguments.clone(),
        };
        state
            .dal
            .append_message(conv_id, MessageRole::ToolUse, "", Some(&meta))
            .await?;
        appended.push(WireMessage {
            role: MessageRole::ToolUse,
            content: String::new(),
            metadata: Some(meta),
            created_at: String::new(),
        });
        parsed.push(ToolCall {
            id: tc.id.clone(),
            name,
            arguments,
        });
    }
    Ok(parsed)
}

async fn send_done(
    tx: &mpsc::Sender<Response>,
    conversation_id: String,
    messages_appended: Vec<WireMessage>,
) -> Result<(), ChatFlow> {
    tx.send(Response::ChatDone {
        conversation_id,
        messages_appended,
    })
    .await?;
    Ok(())
}

/// Convert a row back into the shape the LLM trait expects. Groups adjacent
/// `tool_use` rows that follow an `assistant` row into a single Assistant
/// message with its `tool_calls` attached.
fn stored_to_llm(m: &StoredMessage) -> Result<Message, DbError> {
    match (m.role, m.metadata.as_ref()) {
        (MessageRole::System, None) => Ok(Message::system(&m.content)),
        (MessageRole::User, None) => Ok(Message::user(&m.content)),
        (MessageRole::Assistant, None) => Ok(Message::assistant(&m.content)),
        (
            MessageRole::ToolUse,
            Some(MessageMetadata::ToolUse {
                tool_call_id,
                name,
                arguments,
            }),
        ) => Ok(Message::Assistant {
            content: m.content.clone(),
            tool_calls: vec![ToolCall {
                id: tool_call_id.clone(),
                name: name.clone(),
                arguments: arguments.clone(),
            }],
        }),
        (MessageRole::Tool, Some(MessageMetadata::Tool { tool_call_id })) => Ok(Message::Tool {
            tool_call_id: tool_call_id.clone(),
            content: m.content.clone(),
        }),
        _ => Err(DbError::InvalidMessage {
            reason: format!(
                "cannot convert stored message role={:?} metadata={:?} to LLM message",
                m.role, m.metadata
            ),
        }),
    }
}

/// Internal control-flow type for `handle_chat_inner`. Splits two kinds of
/// failure that need different handling:
/// - `Emit`: a recoverable failure (DB or LLM) that should be surfaced as a
///   `Response::Error` frame without tearing down the connection.
/// - `WriterGone`: the consumer of `tx` has dropped, so there is no one left
///   to send anything to. Propagate up so the server stops reading further
///   requests on this connection.
enum ChatFlow {
    Emit(String),
    WriterGone(SendError<Response>),
}

impl From<DbError> for ChatFlow {
    fn from(err: DbError) -> Self {
        ChatFlow::Emit(err.to_string())
    }
}

impl From<crate::llm::LlmError> for ChatFlow {
    fn from(err: crate::llm::LlmError) -> Self {
        ChatFlow::Emit(err.to_string())
    }
}

impl From<SendError<Response>> for ChatFlow {
    fn from(err: SendError<Response>) -> Self {
        ChatFlow::WriterGone(err)
    }
}

async fn handle_conversation_list(
    state: &ServerState,
    tags: Vec<String>,
    text_query: Option<String>,
    limit: Option<usize>,
) -> Result<Response, DbError> {
    let tag_refs: Vec<&str> = tags.iter().map(String::as_str).collect();
    let filter = ConversationFilter {
        tags: &tag_refs,
        text_query: text_query.as_deref(),
        limit,
    };
    let conversations = state.dal.list_conversations(&filter).await?;
    let mut items = Vec::with_capacity(conversations.len());
    for c in conversations {
        let tags = state.dal.tags_for_conversation(&c.id).await?;
        items.push(ConversationSummary {
            id: c.id,
            title: c.title,
            updated_at: c.updated_at,
            tags,
        });
    }
    Ok(Response::ConversationList { items })
}

async fn handle_conversation_get(state: &ServerState, id: String) -> Result<Response, DbError> {
    let Some(conv) = state.dal.get_conversation(&id).await? else {
        return Err(DbError::NotFound {
            kind: "conversation",
            id: 0,
        });
    };
    let tags = state.dal.tags_for_conversation(&id).await?;
    let messages = state
        .dal
        .messages_for_conversation(&id)
        .await?
        .into_iter()
        .map(stored_to_wire)
        .collect();
    Ok(Response::ConversationGet {
        conversation: ConversationSummary {
            id: conv.id,
            title: conv.title,
            updated_at: conv.updated_at,
            tags,
        },
        messages,
    })
}

pub(crate) fn stored_to_wire(m: StoredMessage) -> WireMessage {
    WireMessage {
        role: m.role,
        content: m.content,
        metadata: m.metadata,
        created_at: m.created_at,
    }
}

async fn handle_document_list(
    state: &ServerState,
    tags: Vec<String>,
    limit: Option<usize>,
) -> Result<Response, DbError> {
    let tag_refs: Vec<&str> = tags.iter().map(String::as_str).collect();
    let filter = DocumentFilter {
        tags: &tag_refs,
        limit,
    };
    let docs = state.dal.list_documents_filtered(&filter).await?;
    let mut items = Vec::with_capacity(docs.len());
    for d in docs {
        let tags = state.dal.tags_for_document(d.id).await?;
        items.push(DocumentSummary {
            id: d.id,
            path: d.path,
            created_at: d.created_at,
            tags,
        });
    }
    Ok(Response::DocumentList { items })
}

/// Streaming `DocumentCreate`: emits `DocumentCreateStart`, zero-or-more
/// `DocumentCreateProgress`, and a terminal `DocumentCreateDone` — or a single
/// `Response::Error` if anything fails. Mirrors the shape of `handle_chat`.
async fn handle_document_create(
    state: &ServerState,
    path: String,
    tags: Vec<String>,
    tx: &mpsc::Sender<Response>,
) -> Result<(), SendError<Response>> {
    // Read + chunk outside the transaction so we can emit Start with the
    // total chunk count before the first embedding call.
    let path_buf = PathBuf::from(&path);
    let text = match ingest::read::read_text_file(&path_buf) {
        Ok(t) => t,
        Err(err) => return tx.send(ingest_err_to_response(&err)).await,
    };
    let chunks = ingest::chunking::chunk(&text);
    if chunks.is_empty() {
        return tx
            .send(Response::Error {
                message: format!("{path:?} is empty"),
            })
            .await;
    }

    // Embed before DocumentCreateStart so we know the full chunk count and any
    // embed error surfaces as a clean Error frame (no partial document row).
    let contents: Vec<String> = chunks.iter().map(|c| c.content.clone()).collect();
    let vectors = match state.llm.embeddings.embed_batch(&contents).await {
        Ok(v) => v,
        Err(err) => {
            return tx
                .send(Response::Error {
                    message: err.to_string(),
                })
                .await;
        }
    };
    if vectors.len() != chunks.len() {
        return tx
            .send(Response::Error {
                message: format!(
                    "embed_batch returned {} vectors for {} chunks",
                    vectors.len(),
                    chunks.len()
                ),
            })
            .await;
    }

    // Persist transactionally. We inline the DAL transaction here so the
    // progress frames can be streamed between chunk inserts.
    if let Err(err) = state.dal.begin().await {
        return tx.send(err_response(err)).await;
    }

    let document_id = match state.dal.create_document(&path).await {
        Ok(id) => id,
        Err(err) => {
            let _ = state.dal.rollback().await;
            let message = friendly_create_document_error(&path, &err);
            return tx.send(Response::Error { message }).await;
        }
    };
    let total = chunks.len();
    if let Err(send_err) = tx
        .send(Response::DocumentCreateStart {
            document_id,
            total_chunks: total,
        })
        .await
    {
        let _ = state.dal.rollback().await;
        return Err(send_err);
    }

    for (index, (chunk, embedding)) in chunks.iter().zip(vectors.iter()).enumerate() {
        if let Err(err) = state
            .dal
            .create_chunk(
                document_id,
                ChunkRange::Bytes {
                    start: chunk.byte_start,
                    end: chunk.byte_end,
                },
                &chunk.content,
                embedding,
            )
            .await
        {
            let _ = state.dal.rollback().await;
            return tx.send(err_response(err)).await;
        }
        if let Err(send_err) = tx
            .send(Response::DocumentCreateProgress {
                document_id,
                index,
                total,
            })
            .await
        {
            let _ = state.dal.rollback().await;
            return Err(send_err);
        }
    }

    for tag in &tags {
        if let Err(err) = state.dal.add_tag(document_id, tag).await {
            let _ = state.dal.rollback().await;
            return tx.send(err_response(err)).await;
        }
    }

    if let Err(err) = state.dal.commit().await {
        return tx.send(err_response(err)).await;
    }

    // Mirror ChunkProgress so test/runtime paths stay uniform.
    let _ = ChunkProgress {
        index: total.saturating_sub(1),
        total,
    };

    tx.send(Response::DocumentCreateDone {
        document_id,
        chunks: total,
    })
    .await
}

async fn handle_document_search(
    state: &ServerState,
    query: String,
    tags: Vec<String>,
    limit: Option<usize>,
) -> Result<Response, anyhow::Error> {
    let limit = limit.unwrap_or(20);
    let vec = state
        .llm
        .embeddings
        .embed(&query)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    let tag_refs: Vec<&str> = tags.iter().map(String::as_str).collect();
    let hits = state
        .dal
        .search_chunks(&vec, limit, &tag_refs)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    // Hydrate paths. Cache per-document lookups to avoid repeats when many
    // chunks share one document.
    let mut results = Vec::with_capacity(hits.len());
    let mut path_cache: std::collections::HashMap<i64, String> = std::collections::HashMap::new();
    for h in hits {
        let path = match path_cache.get(&h.document_id) {
            Some(p) => p.clone(),
            None => {
                let doc = state
                    .dal
                    .get_document(h.document_id)
                    .await
                    .map_err(|e| anyhow::anyhow!(e))?;
                let p = doc.map(|d| d.path).unwrap_or_default();
                path_cache.insert(h.document_id, p.clone());
                p
            }
        };
        let (range_kind, range_start, range_end) = match h.range {
            ChunkRange::Pages { start, end } => ("pages".to_string(), start as i64, end as i64),
            ChunkRange::Bytes { start, end } => ("bytes".to_string(), start as i64, end as i64),
        };
        results.push(DocumentSearchHit {
            document_id: h.document_id,
            path,
            chunk_id: h.chunk_id,
            content: h.content,
            distance: h.distance,
            range_kind,
            range_start,
            range_end,
        });
    }
    Ok(Response::DocumentSearch { results })
}

fn friendly_create_document_error(path: &str, err: &DbError) -> String {
    if let DbError::Query {
        op: "create_document",
        source,
    } = err
    {
        let text = source.to_string();
        if text.contains("UNIQUE") || text.contains("unique") {
            return format!("document already exists at {path:?} — delete the existing one first",);
        }
    }
    err.to_string()
}

fn ingest_err_to_response(err: &IngestError) -> Response {
    Response::Error {
        message: err.to_string(),
    }
}

fn err_response<E: std::fmt::Display>(err: E) -> Response {
    Response::Error {
        message: err.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Dal, MessageMetadata, MessageRole};
    use crate::llm::LlmStack;
    use crate::llm::types::StreamChunk;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn uuid() -> String {
        uuid::Uuid::now_v7().hyphenated().to_string()
    }

    /// Drive `dispatch` against a freshly created channel and return every
    /// frame it emitted, in order. Used in place of the old single-`Response`
    /// return.
    async fn collect_frames(req: Request, state: &ServerState) -> Vec<Response> {
        let (tx, mut rx) = mpsc::channel::<Response>(32);
        let (_, frames) = tokio::join!(
            async {
                dispatch(req, state, &tx).await.unwrap();
                drop(tx);
            },
            async {
                let mut out = Vec::new();
                while let Some(r) = rx.recv().await {
                    out.push(r);
                }
                out
            }
        );
        frames
    }

    /// Helper for tests that expect a single-frame response.
    async fn one_frame(req: Request, state: &ServerState) -> Response {
        let mut frames = collect_frames(req, state).await;
        assert_eq!(frames.len(), 1, "expected exactly one frame: {frames:?}");
        frames.pop().unwrap()
    }

    async fn state_with_dal() -> (ServerState, TempDir) {
        let dir = TempDir::new().unwrap();
        let path: PathBuf = dir.path().join("t.db");
        let dal = Dal::open(&path, "test-model".into(), || async move { Ok(4) })
            .await
            .unwrap();
        let mock = Arc::new(crate::llm::mock::MockProvider::new());
        let state = ServerState {
            dal: Arc::new(dal),
            llm: Arc::new(LlmStack {
                chat: mock.clone(),
                embeddings: mock,
            }),
            tools: Arc::new(crate::tools::ToolRegistry::with_defaults()),
        };
        (state, dir)
    }

    #[tokio::test]
    async fn list_get_delete_round_trip() {
        let (state, _dir) = state_with_dal().await;
        let id = uuid();
        state
            .dal
            .create_conversation(&id, Some("hi"))
            .await
            .unwrap();
        state
            .dal
            .append_message(&id, MessageRole::User, "hello", None)
            .await
            .unwrap();

        let resp = one_frame(
            Request::ConversationList {
                tags: vec![],
                text_query: None,
                limit: None,
            },
            &state,
        )
        .await;
        let Response::ConversationList { items } = resp else {
            panic!("expected ConversationList");
        };
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, id);

        let resp = one_frame(Request::ConversationGet { id: id.clone() }, &state).await;
        let Response::ConversationGet { messages, .. } = resp else {
            panic!("expected ConversationGet");
        };
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, MessageRole::User);

        let resp = one_frame(Request::ConversationDelete { id: id.clone() }, &state).await;
        assert!(matches!(resp, Response::Ok));
        let resp = one_frame(Request::ConversationGet { id }, &state).await;
        assert!(matches!(resp, Response::Error { .. }));
    }

    #[tokio::test]
    async fn tag_crud_via_dispatch() {
        let (state, _dir) = state_with_dal().await;
        let id = uuid();
        state.dal.create_conversation(&id, None).await.unwrap();

        let resp = one_frame(
            Request::ConversationAddTag {
                id: id.clone(),
                tag: "foo".into(),
            },
            &state,
        )
        .await;
        assert!(matches!(resp, Response::Ok));

        let resp = one_frame(Request::ConversationTags { id: id.clone() }, &state).await;
        let Response::ConversationTags { tags } = resp else {
            panic!("expected tags");
        };
        assert_eq!(tags, vec!["foo"]);

        let resp = one_frame(Request::TagList, &state).await;
        let Response::TagList { tags } = resp else {
            panic!("expected TagList");
        };
        assert_eq!(tags, vec!["foo"]);

        let resp = one_frame(
            Request::ConversationRemoveTag {
                id: id.clone(),
                tag: "foo".into(),
            },
            &state,
        )
        .await;
        assert!(matches!(resp, Response::Ok));

        let resp = one_frame(Request::ConversationTags { id }, &state).await;
        let Response::ConversationTags { tags } = resp else {
            panic!("expected tags");
        };
        assert!(tags.is_empty());
    }

    #[tokio::test]
    async fn list_with_tag_and_text_filter() {
        let (state, _dir) = state_with_dal().await;
        let a = uuid();
        let b = uuid();
        state.dal.create_conversation(&a, None).await.unwrap();
        state.dal.create_conversation(&b, None).await.unwrap();
        state.dal.add_conversation_tag(&a, "work").await.unwrap();
        state
            .dal
            .append_message(&a, MessageRole::User, "hello rust", None)
            .await
            .unwrap();
        state
            .dal
            .append_message(&b, MessageRole::User, "hello python", None)
            .await
            .unwrap();

        let resp = one_frame(
            Request::ConversationList {
                tags: vec!["work".into()],
                text_query: Some("rust".into()),
                limit: None,
            },
            &state,
        )
        .await;
        let Response::ConversationList { items } = resp else {
            panic!("expected ConversationList");
        };
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, a);
    }

    /// Set up a state with a mock provider preloaded with one or more streams
    /// of chunks — each element of `streams` is consumed by one call to
    /// `chat_stream`, in order. Passing a single stream matches the common
    /// single-turn case; multi-element lists exercise the tool-call loop.
    async fn make_chat_state(
        streams: Vec<Vec<StreamChunk>>,
    ) -> (ServerState, Arc<crate::llm::mock::MockProvider>, TempDir) {
        let dir = TempDir::new().unwrap();
        let path: PathBuf = dir.path().join("t.db");
        let dal = Dal::open(&path, "test-model".into(), || async move { Ok(4) })
            .await
            .unwrap();
        let mock = Arc::new(crate::llm::mock::MockProvider::new());
        for chunks in streams {
            mock.push_stream(chunks);
        }
        let state = ServerState {
            dal: Arc::new(dal),
            llm: Arc::new(LlmStack {
                chat: mock.clone(),
                embeddings: mock.clone(),
            }),
            tools: Arc::new(crate::tools::ToolRegistry::with_defaults()),
        };
        (state, mock, dir)
    }

    /// Pull `ChatDone { conversation_id, messages_appended }` out of a
    /// collected frame sequence, asserting the shape along the way.
    fn assert_chat_stream_shape(frames: &[Response]) -> (String, Vec<WireMessage>) {
        assert!(!frames.is_empty(), "expected at least one frame");
        let conv_id = match &frames[0] {
            Response::ChatStart { conversation_id } => conversation_id.clone(),
            other => panic!("first frame should be ChatStart, got {other:?}"),
        };
        match frames.last().unwrap() {
            Response::ChatDone {
                conversation_id,
                messages_appended,
            } => {
                assert_eq!(conversation_id, &conv_id);
                (conv_id, messages_appended.clone())
            }
            other => panic!("last frame should be ChatDone, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn chat_creates_conversation_and_persists_turns() {
        let (state, _mock, _dir) = make_chat_state(vec![vec![
            StreamChunk::Text("hi ".into()),
            StreamChunk::Text("there".into()),
        ]])
        .await;

        let frames = collect_frames(
            Request::Chat {
                conversation_id: None,
                message: "hello".into(),
            },
            &state,
        )
        .await;

        // Expect: ChatStart, two ChatDelta, ChatDone.
        assert_eq!(frames.len(), 4, "frames: {frames:?}");
        assert!(matches!(frames[1], Response::ChatDelta { ref text, .. } if text == "hi "));
        assert!(matches!(frames[2], Response::ChatDelta { ref text, .. } if text == "there"));

        let (conv_id, appended) = assert_chat_stream_shape(&frames);
        assert_eq!(appended.len(), 1);
        assert_eq!(appended[0].role, MessageRole::Assistant);
        assert_eq!(appended[0].content, "hi there");

        let msgs = state.dal.messages_for_conversation(&conv_id).await.unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, MessageRole::User);
        assert_eq!(msgs[0].content, "hello");
        assert_eq!(msgs[1].role, MessageRole::Assistant);
        assert_eq!(msgs[1].content, "hi there");
    }

    #[tokio::test]
    async fn chat_reuses_conversation_and_includes_history() {
        let (state, mock, _dir) =
            make_chat_state(vec![vec![StreamChunk::Text("second reply".into())]]).await;
        let id = uuid();
        state.dal.create_conversation(&id, None).await.unwrap();
        state
            .dal
            .append_message(&id, MessageRole::User, "first", None)
            .await
            .unwrap();
        state
            .dal
            .append_message(&id, MessageRole::Assistant, "first reply", None)
            .await
            .unwrap();

        let frames = collect_frames(
            Request::Chat {
                conversation_id: Some(id.clone()),
                message: "follow-up".into(),
            },
            &state,
        )
        .await;
        assert_chat_stream_shape(&frames);

        let reqs = mock.received_requests.lock().unwrap();
        assert_eq!(reqs.len(), 1);
        // history (user, assistant) + new user turn
        assert_eq!(reqs[0].messages.len(), 3);
    }

    #[tokio::test]
    async fn chat_invokes_tool_then_continues_with_text() {
        // Stream #1: model asks for `add({a: 2, b: 3})`.
        // Stream #2: model produces final text ("sum is 5").
        // End-to-end: ChatStart, 2x ChatToolCallDelta, ChatToolResult("5"),
        // ChatDelta("sum is 5"), ChatDone.
        let (state, _mock, _dir) = make_chat_state(vec![
            vec![
                StreamChunk::ToolCallDelta {
                    id: "c1".into(),
                    name: Some("add".into()),
                    arguments_delta: "{\"a\":2,".into(),
                },
                StreamChunk::ToolCallDelta {
                    id: "c1".into(),
                    name: None,
                    arguments_delta: "\"b\":3}".into(),
                },
            ],
            vec![StreamChunk::Text("sum is 5".into())],
        ])
        .await;

        let frames = collect_frames(
            Request::Chat {
                conversation_id: None,
                message: "what is 2+3?".into(),
            },
            &state,
        )
        .await;

        assert_eq!(frames.len(), 6, "frames: {frames:?}");
        assert!(matches!(frames[0], Response::ChatStart { .. }));
        let tool_delta_count = frames
            .iter()
            .filter(|f| matches!(f, Response::ChatToolCallDelta { .. }))
            .count();
        assert_eq!(tool_delta_count, 2);
        match &frames[3] {
            Response::ChatToolResult { id, content, .. } => {
                assert_eq!(id, "c1");
                assert_eq!(content, "5");
            }
            other => panic!("expected ChatToolResult at [3], got {other:?}"),
        }
        assert!(matches!(
            frames[4],
            Response::ChatDelta { ref text, .. } if text == "sum is 5"
        ));

        let (conv_id, appended) = assert_chat_stream_shape(&frames);
        // appended: ToolUse + Tool + Assistant("sum is 5")
        assert_eq!(appended.len(), 3);
        assert_eq!(appended[0].role, MessageRole::ToolUse);
        assert_eq!(appended[1].role, MessageRole::Tool);
        assert_eq!(appended[1].content, "5");
        assert_eq!(appended[2].role, MessageRole::Assistant);
        assert_eq!(appended[2].content, "sum is 5");

        // DB rows: User + ToolUse + Tool + Assistant.
        let rows = state.dal.messages_for_conversation(&conv_id).await.unwrap();
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].role, MessageRole::User);
        assert_eq!(rows[1].role, MessageRole::ToolUse);
        assert_eq!(rows[2].role, MessageRole::Tool);
        assert_eq!(rows[2].content, "5");
        assert_eq!(rows[3].role, MessageRole::Assistant);
    }

    #[tokio::test]
    async fn chat_error_before_start_surfaces_as_error_frame() {
        let (state, _dir) = state_with_dal().await;
        // MockProvider with no queued stream returns an error from chat_stream
        // itself — actually, looking at mock.rs, an empty stream responses
        // queue yields an empty Vec then appends Done, so it completes with no
        // chunks. Test that path instead: no assistant text, no tool calls →
        // messages_appended is empty.
        let frames = collect_frames(
            Request::Chat {
                conversation_id: None,
                message: "hello".into(),
            },
            &state,
        )
        .await;
        let (_, appended) = assert_chat_stream_shape(&frames);
        assert!(appended.is_empty(), "expected no appended rows");
    }

    #[tokio::test]
    async fn chat_tool_invocation_error_becomes_result_content() {
        // Model asks `add` with the wrong argument type. The tool rejects it
        // and the handler records the error string as the tool result rather
        // than tearing down the turn.
        let (state, _mock, _dir) = make_chat_state(vec![
            vec![StreamChunk::ToolCallDelta {
                id: "c1".into(),
                name: Some("add".into()),
                arguments_delta: "{\"a\":\"two\",\"b\":3}".into(),
            }],
            vec![StreamChunk::Text("sorry".into())],
        ])
        .await;

        let frames = collect_frames(
            Request::Chat {
                conversation_id: None,
                message: "use a tool".into(),
            },
            &state,
        )
        .await;

        let tool_result_content = frames
            .iter()
            .find_map(|f| match f {
                Response::ChatToolResult { content, .. } => Some(content.clone()),
                _ => None,
            })
            .expect("expected a ChatToolResult frame");
        assert!(
            tool_result_content.starts_with("error:"),
            "got {tool_result_content:?}"
        );

        let (_, appended) = assert_chat_stream_shape(&frames);
        // appended: ToolUse + Tool("error: ...") + Assistant("sorry").
        assert_eq!(appended.len(), 3);
        assert_eq!(appended[1].role, MessageRole::Tool);
        assert!(appended[1].content.starts_with("error:"));
    }

    #[tokio::test]
    async fn chat_malformed_tool_args_json_stored_as_string() {
        // The model emits unparseable JSON for tool args. We keep the raw
        // string on the ToolUse row (so conversation replay is faithful) and
        // the tool invocation fails with InvalidArguments → error-prefixed
        // result content.
        let (state, _mock, _dir) = make_chat_state(vec![
            vec![StreamChunk::ToolCallDelta {
                id: "c1".into(),
                name: Some("add".into()),
                arguments_delta: "not-json".into(),
            }],
            vec![StreamChunk::Text("ok".into())],
        ])
        .await;

        let frames = collect_frames(
            Request::Chat {
                conversation_id: None,
                message: "use a tool".into(),
            },
            &state,
        )
        .await;
        let (_, appended) = assert_chat_stream_shape(&frames);
        match &appended[0].metadata {
            Some(MessageMetadata::ToolUse { arguments, .. }) => {
                assert_eq!(arguments, &serde_json::Value::String("not-json".into()));
            }
            other => panic!("expected ToolUse metadata, got {other:?}"),
        }
        assert_eq!(appended[1].role, MessageRole::Tool);
        assert!(appended[1].content.starts_with("error:"));
    }

    #[tokio::test]
    async fn conversation_get_returns_tool_use_metadata() {
        let (state, _dir) = state_with_dal().await;
        let id = uuid();
        state.dal.create_conversation(&id, None).await.unwrap();
        state
            .dal
            .append_message(
                &id,
                MessageRole::ToolUse,
                "",
                Some(&MessageMetadata::ToolUse {
                    tool_call_id: "c1".into(),
                    name: "search".into(),
                    arguments: serde_json::json!({"q": "rust"}),
                }),
            )
            .await
            .unwrap();

        let resp = one_frame(Request::ConversationGet { id }, &state).await;
        let Response::ConversationGet { messages, .. } = resp else {
            panic!("expected ConversationGet");
        };
        assert_eq!(messages.len(), 1);
        assert!(matches!(
            messages[0].metadata,
            Some(MessageMetadata::ToolUse { .. })
        ));
    }
}
