//! Translate between our provider-agnostic types and `rig`'s message/content
//! types. Kept private so the rest of the crate never needs `rig::...` in
//! scope.

use anyhow::anyhow;
use rig::OneOrMany;
use rig::completion::{self, ToolDefinition};
use rig::message::{
    self, AssistantContent, Text, ToolCall as RigToolCall, ToolFunction, ToolResult,
    ToolResultContent, UserContent,
};

use super::error::LlmError;
use super::types::{ChatResponse, Message, Tool, ToolCall};

/// Convert our messages into rig's. Our `Tool` role maps to a user-role
/// message carrying a `ToolResult` — that's how rig models tool replies.
pub(super) fn messages_to_rig(messages: &[Message]) -> Vec<message::Message> {
    messages.iter().map(message_to_rig).collect()
}

fn message_to_rig(msg: &Message) -> message::Message {
    match msg {
        Message::System { content } => message::Message::System {
            content: content.clone(),
        },
        Message::User { content } => message::Message::User {
            content: OneOrMany::one(UserContent::Text(Text {
                text: content.clone(),
            })),
        },
        Message::Assistant {
            content,
            tool_calls,
        } => {
            let mut items: Vec<AssistantContent> = Vec::new();
            if !content.is_empty() {
                items.push(AssistantContent::Text(Text {
                    text: content.clone(),
                }));
            }
            for tc in tool_calls {
                items.push(AssistantContent::ToolCall(RigToolCall {
                    id: tc.id.clone(),
                    call_id: None,
                    function: ToolFunction {
                        name: tc.name.clone(),
                        arguments: tc.arguments.clone(),
                    },
                    signature: None,
                    additional_params: None,
                }));
            }
            if items.is_empty() {
                items.push(AssistantContent::Text(Text {
                    text: String::new(),
                }));
            }
            message::Message::Assistant {
                id: None,
                content: OneOrMany::many(items)
                    .expect("at least one assistant content item ensured above"),
            }
        }
        Message::Tool {
            tool_call_id,
            content,
        } => message::Message::User {
            content: OneOrMany::one(UserContent::ToolResult(ToolResult {
                id: tool_call_id.clone(),
                call_id: None,
                content: OneOrMany::one(ToolResultContent::Text(Text {
                    text: content.clone(),
                })),
            })),
        },
    }
}

pub(super) fn tools_to_rig(tools: &[Tool]) -> Vec<ToolDefinition> {
    tools
        .iter()
        .map(|t| ToolDefinition {
            name: t.name.clone(),
            description: t.description.clone(),
            parameters: t.parameters.clone(),
        })
        .collect()
}

/// Flatten rig's assistant response into our `ChatResponse` (text and tool
/// calls only — reasoning/images are dropped for now).
pub(super) fn rig_choice_to_response(
    choice: &OneOrMany<AssistantContent>,
) -> Result<ChatResponse, LlmError> {
    let mut text = String::new();
    let mut tool_calls = Vec::new();

    for item in choice.iter() {
        match item {
            AssistantContent::Text(t) => text.push_str(&t.text),
            AssistantContent::ToolCall(tc) => tool_calls.push(ToolCall {
                id: tc.id.clone(),
                name: tc.function.name.clone(),
                arguments: tc.function.arguments.clone(),
            }),
            AssistantContent::Reasoning(_) | AssistantContent::Image(_) => {}
        }
    }

    Ok(ChatResponse {
        text,
        tool_calls,
        structured: None,
    })
}

/// Build a `rig::completion::CompletionRequest` from our request. Splits the
/// last message off as the "prompt" that rig requires (and the builder
/// appends back to chat_history at build time). If the last message is a
/// `Tool` message (a result returning to the model), we still hand it as the
/// prompt — rig wraps it as a user message with the tool result.
pub(super) fn build_rig_request(
    req: &super::types::ChatRequest,
    system_prompt_for_json_schema: Option<&str>,
) -> Result<RigRequestInputs, LlmError> {
    if req.messages.is_empty() {
        return Err(LlmError::ConfigInvalid(
            "chat request requires at least one message".into(),
        ));
    }

    let mut rig_messages = messages_to_rig(&req.messages);
    if let Some(extra_system) = system_prompt_for_json_schema {
        rig_messages.insert(
            0,
            message::Message::System {
                content: extra_system.to_string(),
            },
        );
    }

    let prompt = rig_messages.pop().expect("non-empty per check above");

    let chat_history: Vec<message::Message> = rig_messages;
    Ok(RigRequestInputs {
        prompt,
        chat_history,
        tools: tools_to_rig(&req.tools),
        temperature: req.temperature,
        max_tokens: req.max_tokens,
    })
}

pub(super) struct RigRequestInputs {
    pub prompt: message::Message,
    pub chat_history: Vec<message::Message>,
    pub tools: Vec<ToolDefinition>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u64>,
}

pub(super) fn completion_error_to_llm(err: completion::CompletionError) -> LlmError {
    LlmError::Provider(anyhow!(err))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::types::{Message, ToolCall};
    use serde_json::json;

    #[test]
    fn our_user_message_roundtrips_to_rig_user() {
        let msgs = vec![Message::user("hello")];
        let rig = messages_to_rig(&msgs);
        assert_eq!(rig.len(), 1);
        matches!(rig[0], message::Message::User { .. });
    }

    #[test]
    fn our_system_message_roundtrips_to_rig_system() {
        let msgs = vec![Message::system("you are helpful")];
        let rig = messages_to_rig(&msgs);
        match &rig[0] {
            message::Message::System { content } => assert_eq!(content, "you are helpful"),
            other => panic!("expected System, got {other:?}"),
        }
    }

    #[test]
    fn our_assistant_with_tool_calls_roundtrips() {
        let msgs = vec![Message::Assistant {
            content: "let me check".into(),
            tool_calls: vec![ToolCall {
                id: "call-1".into(),
                name: "get_weather".into(),
                arguments: json!({"city": "NYC"}),
            }],
        }];
        let rig = messages_to_rig(&msgs);
        match &rig[0] {
            message::Message::Assistant { content, .. } => {
                let items: Vec<_> = content.iter().collect();
                assert_eq!(items.len(), 2, "text + tool call");
            }
            other => panic!("expected Assistant, got {other:?}"),
        }
    }

    #[test]
    fn our_tool_message_maps_to_rig_user_tool_result() {
        let msgs = vec![Message::tool("call-1", "72 degrees")];
        let rig = messages_to_rig(&msgs);
        match &rig[0] {
            message::Message::User { content } => {
                let items: Vec<_> = content.iter().collect();
                assert_eq!(items.len(), 1);
                matches!(items[0], UserContent::ToolResult(_));
            }
            other => panic!("expected User, got {other:?}"),
        }
    }

    #[test]
    fn rig_choice_to_response_extracts_text_and_tool_calls() {
        let choice = OneOrMany::many(vec![
            AssistantContent::Text(Text {
                text: "sure, ".into(),
            }),
            AssistantContent::Text(Text {
                text: "calling tool".into(),
            }),
            AssistantContent::ToolCall(RigToolCall {
                id: "call-42".into(),
                call_id: None,
                function: ToolFunction {
                    name: "search".into(),
                    arguments: json!({"q": "rust"}),
                },
                signature: None,
                additional_params: None,
            }),
        ])
        .unwrap();

        let resp = rig_choice_to_response(&choice).unwrap();
        assert_eq!(resp.text, "sure, calling tool");
        assert_eq!(resp.tool_calls.len(), 1);
        assert_eq!(resp.tool_calls[0].name, "search");
        assert_eq!(resp.tool_calls[0].arguments, json!({"q": "rust"}));
    }
}
