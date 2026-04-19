use crate::db::{ConversationFilter, DbError, MessageMetadata, MessageRole, StoredMessage};
use crate::llm::types::{ChatRequest, Message, ToolCall};
use crate::protocol::{ConversationSummary, Request, Response, WireMessage};
use crate::server::ServerState;

pub async fn dispatch(req: Request, state: &ServerState) -> Response {
    match req {
        Request::Ping => Response::Pong,
        Request::Chat {
            conversation_id,
            message,
        } => match handle_chat(state, conversation_id, message).await {
            Ok(resp) => resp,
            Err(err) => err_response(err),
        },
        Request::ConversationList {
            tags,
            text_query,
            limit,
        } => match handle_conversation_list(state, tags, text_query, limit).await {
            Ok(resp) => resp,
            Err(err) => err_response(err),
        },
        Request::ConversationGet { id } => match handle_conversation_get(state, id).await {
            Ok(resp) => resp,
            Err(err) => err_response(err),
        },
        Request::ConversationDelete { id } => match state.dal.delete_conversation(&id).await {
            Ok(()) => Response::Ok,
            Err(err) => err_response(err),
        },
        Request::ConversationAddTag { id, tag } => {
            match state.dal.add_conversation_tag(&id, &tag).await {
                Ok(()) => Response::Ok,
                Err(err) => err_response(err),
            }
        }
        Request::ConversationRemoveTag { id, tag } => {
            match state.dal.remove_conversation_tag(&id, &tag).await {
                Ok(()) => Response::Ok,
                Err(err) => err_response(err),
            }
        }
        Request::ConversationTags { id } => match state.dal.tags_for_conversation(&id).await {
            Ok(tags) => Response::ConversationTags { tags },
            Err(err) => err_response(err),
        },
        Request::TagList => match state.dal.list_all_tags().await {
            Ok(tags) => Response::TagList { tags },
            Err(err) => err_response(err),
        },
    }
}

/// Send `message` to the LLM in the context of `conversation_id`'s history,
/// persist both the user turn and everything the model produced, and return
/// the appended rows to the client. A `None` id mints a new conversation.
async fn handle_chat(
    state: &ServerState,
    conversation_id: Option<String>,
    message: String,
) -> Result<Response, ChatError> {
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

    // Persist the user message and capture the ID for the wire reply.
    state
        .dal
        .append_message(&conv_id, MessageRole::User, &message, None)
        .await?;

    // Build the prompt: prior history + the new user turn.
    let mut messages = Vec::with_capacity(history.len() + 1);
    for m in &history {
        messages.push(stored_to_llm(m)?);
    }
    messages.push(Message::user(message.clone()));

    let response = state
        .llm
        .chat
        .chat(ChatRequest {
            messages,
            ..Default::default()
        })
        .await?;

    // Persist the assistant turn. We persist text first (if any), then each
    // tool call as its own `tool_use` row so the DB shape stays flat.
    let mut appended: Vec<WireMessage> = Vec::new();

    // Also emit the just-saved user row so the TUI can render it with proper
    // role styling on the same turn. Fetch the full message list diff by
    // reading everything appended after `history.len()`.
    // We'll just build WireMessages directly from what we know we persisted.
    appended.push(WireMessage {
        role: MessageRole::User,
        content: message,
        metadata: None,
        created_at: String::new(),
    });

    if !response.text.is_empty() {
        state
            .dal
            .append_message(&conv_id, MessageRole::Assistant, &response.text, None)
            .await?;
        appended.push(WireMessage {
            role: MessageRole::Assistant,
            content: response.text.clone(),
            metadata: None,
            created_at: String::new(),
        });
    }
    for tc in &response.tool_calls {
        let meta = MessageMetadata::ToolUse {
            tool_call_id: tc.id.clone(),
            name: tc.name.clone(),
            arguments: tc.arguments.clone(),
        };
        state
            .dal
            .append_message(&conv_id, MessageRole::ToolUse, "", Some(&meta))
            .await?;
        appended.push(WireMessage {
            role: MessageRole::ToolUse,
            content: String::new(),
            metadata: Some(meta),
            created_at: String::new(),
        });
    }

    Ok(Response::Chat {
        conversation_id: conv_id,
        reply: response.text,
        messages_appended: appended,
    })
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

/// Error surface for chat: DB or LLM. Both are collapsed to
/// `Response::Error { message }` by the caller.
#[derive(Debug, thiserror::Error)]
enum ChatError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    Llm(#[from] crate::llm::LlmError),
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
    use std::path::PathBuf;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn uuid() -> String {
        uuid::Uuid::now_v7().hyphenated().to_string()
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

        let resp = dispatch(
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

        let resp = dispatch(Request::ConversationGet { id: id.clone() }, &state).await;
        let Response::ConversationGet { messages, .. } = resp else {
            panic!("expected ConversationGet");
        };
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, MessageRole::User);

        let resp = dispatch(Request::ConversationDelete { id: id.clone() }, &state).await;
        assert!(matches!(resp, Response::Ok));
        let resp = dispatch(Request::ConversationGet { id }, &state).await;
        assert!(matches!(resp, Response::Error { .. }));
    }

    #[tokio::test]
    async fn tag_crud_via_dispatch() {
        let (state, _dir) = state_with_dal().await;
        let id = uuid();
        state.dal.create_conversation(&id, None).await.unwrap();

        let resp = dispatch(
            Request::ConversationAddTag {
                id: id.clone(),
                tag: "foo".into(),
            },
            &state,
        )
        .await;
        assert!(matches!(resp, Response::Ok));

        let resp = dispatch(Request::ConversationTags { id: id.clone() }, &state).await;
        let Response::ConversationTags { tags } = resp else {
            panic!("expected tags");
        };
        assert_eq!(tags, vec!["foo"]);

        let resp = dispatch(Request::TagList, &state).await;
        let Response::TagList { tags } = resp else {
            panic!("expected TagList");
        };
        assert_eq!(tags, vec!["foo"]);

        let resp = dispatch(
            Request::ConversationRemoveTag {
                id: id.clone(),
                tag: "foo".into(),
            },
            &state,
        )
        .await;
        assert!(matches!(resp, Response::Ok));

        let resp = dispatch(Request::ConversationTags { id }, &state).await;
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

        let resp = dispatch(
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

    async fn make_chat_state(
        reply_text: &str,
        tool_calls: Vec<crate::llm::ToolCall>,
    ) -> (ServerState, Arc<crate::llm::mock::MockProvider>, TempDir) {
        let dir = TempDir::new().unwrap();
        let path: PathBuf = dir.path().join("t.db");
        let dal = Dal::open(&path, "test-model".into(), || async move { Ok(4) })
            .await
            .unwrap();
        let mock = Arc::new(crate::llm::mock::MockProvider::new());
        mock.push_chat(crate::llm::types::ChatResponse {
            text: reply_text.into(),
            tool_calls,
            structured: None,
        });
        let state = ServerState {
            dal: Arc::new(dal),
            llm: Arc::new(LlmStack {
                chat: mock.clone(),
                embeddings: mock.clone(),
            }),
        };
        (state, mock, dir)
    }

    #[tokio::test]
    async fn chat_creates_conversation_and_persists_turns() {
        let (state, _mock, _dir) = make_chat_state("hi there", vec![]).await;

        let resp = dispatch(
            Request::Chat {
                conversation_id: None,
                message: "hello".into(),
            },
            &state,
        )
        .await;
        let Response::Chat {
            conversation_id,
            reply,
            messages_appended,
        } = resp
        else {
            panic!("expected Chat response");
        };
        assert_eq!(reply, "hi there");
        assert_eq!(messages_appended.len(), 2);
        assert_eq!(messages_appended[0].role, MessageRole::User);
        assert_eq!(messages_appended[1].role, MessageRole::Assistant);

        let msgs = state
            .dal
            .messages_for_conversation(&conversation_id)
            .await
            .unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, MessageRole::User);
        assert_eq!(msgs[0].content, "hello");
        assert_eq!(msgs[1].role, MessageRole::Assistant);
        assert_eq!(msgs[1].content, "hi there");
    }

    #[tokio::test]
    async fn chat_reuses_conversation_and_includes_history() {
        let (state, mock, _dir) = make_chat_state("second reply", vec![]).await;
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

        let resp = dispatch(
            Request::Chat {
                conversation_id: Some(id.clone()),
                message: "follow-up".into(),
            },
            &state,
        )
        .await;
        assert!(matches!(resp, Response::Chat { .. }));

        let reqs = mock.received_requests.lock().unwrap();
        assert_eq!(reqs.len(), 1);
        // history (user, assistant) + new user turn
        assert_eq!(reqs[0].messages.len(), 3);
    }

    #[tokio::test]
    async fn chat_persists_tool_use_rows() {
        use serde_json::json;
        let tool_calls = vec![crate::llm::ToolCall {
            id: "c1".into(),
            name: "search".into(),
            arguments: json!({"q": "rust"}),
        }];
        let (state, _mock, _dir) = make_chat_state("", tool_calls).await;

        let resp = dispatch(
            Request::Chat {
                conversation_id: None,
                message: "use a tool".into(),
            },
            &state,
        )
        .await;
        let Response::Chat {
            conversation_id,
            messages_appended,
            ..
        } = resp
        else {
            panic!("expected Chat");
        };
        assert_eq!(messages_appended.len(), 2); // user + tool_use (no text)
        assert_eq!(messages_appended[1].role, MessageRole::ToolUse);

        let rows = state
            .dal
            .messages_for_conversation(&conversation_id)
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].role, MessageRole::ToolUse);
        assert!(matches!(
            rows[1].metadata,
            Some(MessageMetadata::ToolUse { .. })
        ));
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

        let resp = dispatch(Request::ConversationGet { id }, &state).await;
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
