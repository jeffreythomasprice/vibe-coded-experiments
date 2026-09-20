use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::llm::LlmError;
use crate::llm::message::{Content, Image, Message, Role};
use crate::llm::provider::{ChatOptions, ChatRequest, ChatResponse, Provider, StopReason, Usage};
use crate::llm::tool::{ToolCall, ToolDef};

const PROVIDER_NAME: &str = "ollama";

pub struct OllamaProvider {
    client: reqwest::Client,
    base_url: String,
}

impl OllamaProvider {
    pub fn new(base_url: impl Into<String>, timeout: Duration) -> Self {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .expect("reqwest client with a plain timeout is always buildable");
        Self {
            client,
            base_url: base_url.into(),
        }
    }
}

#[async_trait]
impl Provider for OllamaProvider {
    fn name(&self) -> &'static str {
        PROVIDER_NAME
    }

    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, LlmError> {
        let url = format!("{}/api/chat", self.base_url);
        let body = chat_body(request);

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|source| LlmError::Request {
                url: url.clone(),
                source: Box::new(source),
            })?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(LlmError::Status {
                provider: PROVIDER_NAME,
                status: status.as_u16(),
                url,
                body,
            });
        }

        let bytes = response.bytes().await.map_err(|source| LlmError::Request {
            url: url.clone(),
            source: Box::new(source),
        })?;
        let raw: RawChatResponse =
            serde_json::from_slice(&bytes).map_err(|source| LlmError::Decode {
                provider: PROVIDER_NAME,
                url,
                source,
            })?;

        chat_response(raw)
    }
}

fn chat_body(request: &ChatRequest) -> Value {
    let messages: Vec<Value> = request.messages.iter().map(message_to_wire).collect();
    let mut body = json!({
        "model": request.model,
        "messages": messages,
        "stream": false,
    });

    if !request.tools.is_empty() {
        body["tools"] = Value::Array(request.tools.iter().map(tool_to_wire).collect());
    }

    let options = options_to_wire(&request.options);
    if options.as_object().is_some_and(|map| !map.is_empty()) {
        body["options"] = options;
    }

    body
}

fn message_to_wire(message: &Message) -> Value {
    let mut body = json!({
        "role": role_to_wire(message.role),
        "content": message.text(),
    });

    let images: Vec<Value> = message.images().map(Image::to_base64).map(Value::String).collect();
    if !images.is_empty() {
        body["images"] = Value::Array(images);
    }

    if !message.tool_calls.is_empty() {
        body["tool_calls"] = Value::Array(message.tool_calls.iter().map(tool_call_to_wire).collect());
    }

    if let Some(name) = &message.tool_name {
        body["tool_name"] = Value::String(name.clone());
    }

    body
}

fn role_to_wire(role: Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
    }
}

fn tool_call_to_wire(call: &ToolCall) -> Value {
    json!({
        "function": {
            "name": call.name,
            "arguments": call.arguments,
        }
    })
}

fn tool_to_wire(def: &ToolDef) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": def.name,
            "description": def.description,
            "parameters": def.schema,
        }
    })
}

fn options_to_wire(options: &ChatOptions) -> Value {
    let mut map = serde_json::Map::new();
    if let Some(temperature) = options.temperature {
        map.insert("temperature".to_owned(), json!(temperature));
    }
    if let Some(top_p) = options.top_p {
        map.insert("top_p".to_owned(), json!(top_p));
    }
    if let Some(seed) = options.seed {
        map.insert("seed".to_owned(), json!(seed));
    }
    if let Some(max_tokens) = options.max_tokens {
        map.insert("num_predict".to_owned(), json!(max_tokens));
    }
    if !options.stop.is_empty() {
        map.insert("stop".to_owned(), json!(options.stop));
    }
    Value::Object(map)
}

#[derive(Debug, Deserialize)]
struct RawChatResponse {
    message: RawMessage,
    #[serde(default)]
    done_reason: Option<String>,
    #[serde(default)]
    prompt_eval_count: u32,
    #[serde(default)]
    eval_count: u32,
}

#[derive(Debug, Deserialize)]
struct RawMessage {
    #[serde(default)]
    content: String,
    #[serde(default)]
    tool_calls: Vec<RawToolCall>,
}

#[derive(Debug, Deserialize)]
struct RawToolCall {
    function: RawFunctionCall,
}

#[derive(Debug, Deserialize)]
struct RawFunctionCall {
    name: String,
    #[serde(default)]
    arguments: Value,
}

/// Ollama's `tool_calls` carry no id (unlike OpenAI) and `arguments` is already a
/// parsed object rather than a JSON-encoded string, so ids are synthesized here to
/// give the neutral `ToolCall` a uniform shape across providers.
fn chat_response(raw: RawChatResponse) -> Result<ChatResponse, LlmError> {
    let tool_calls: Vec<ToolCall> = raw
        .message
        .tool_calls
        .into_iter()
        .enumerate()
        .map(|(index, call)| ToolCall {
            id: format!("call_{index}"),
            name: call.function.name,
            arguments: call.function.arguments,
        })
        .collect();

    let stop_reason = if !tool_calls.is_empty() {
        StopReason::ToolCalls
    } else {
        match raw.done_reason.as_deref() {
            Some("stop") | None => StopReason::Stop,
            Some("length") => StopReason::Length,
            Some(other) => StopReason::Other(other.to_owned()),
        }
    };

    let message = Message {
        role: Role::Assistant,
        content: vec![Content::text(raw.message.content)],
        tool_calls,
        tool_call_id: None,
        tool_name: None,
    };

    Ok(ChatResponse {
        message,
        stop_reason,
        usage: Usage {
            prompt_tokens: raw.prompt_eval_count,
            completion_tokens: raw.eval_count,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::message::MediaType;
    use crate::llm::tool::ToolDef;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn request(messages: Vec<Message>) -> ChatRequest {
        ChatRequest {
            model: "qwen3:4b".to_owned(),
            messages,
            tools: Vec::new(),
            options: ChatOptions::default(),
        }
    }

    #[test]
    fn chat_body_omits_tools_when_registry_is_empty() {
        let body = chat_body(&request(vec![Message::user("hi")]));
        assert_eq!(body["stream"], false);
        assert!(body.get("tools").is_none());
        assert!(body.get("options").is_none());
    }

    #[test]
    fn chat_body_includes_tool_definitions() {
        let mut req = request(vec![Message::user("hi")]);
        req.tools.push(ToolDef {
            name: "get_weather".to_owned(),
            description: "look up weather".to_owned(),
            schema: json!({"type": "object", "properties": {}}),
        });
        let body = chat_body(&req);
        assert_eq!(body["tools"][0]["type"], "function");
        assert_eq!(body["tools"][0]["function"]["name"], "get_weather");
        assert_eq!(body["tools"][0]["function"]["parameters"]["type"], "object");
    }

    #[test]
    fn message_to_wire_joins_text_and_base64_encodes_images() {
        let image = Image {
            media_type: MediaType::Png,
            data: vec![1, 2, 3],
        };
        let msg = Message::user_with_images("describe this", vec![image.clone()]);
        let wire = message_to_wire(&msg);
        assert_eq!(wire["content"], "describe this");
        assert_eq!(wire["images"][0], image.to_base64());
    }

    #[test]
    fn message_to_wire_carries_tool_name_on_tool_results() {
        let call = ToolCall {
            id: "call_0".to_owned(),
            name: "get_weather".to_owned(),
            arguments: json!({}),
        };
        let msg = Message::tool_result(&call, vec![Content::text("72F and sunny")]);
        let wire = message_to_wire(&msg);
        assert_eq!(wire["role"], "tool");
        assert_eq!(wire["tool_name"], "get_weather");
        assert_eq!(wire["content"], "72F and sunny");
    }

    #[test]
    fn options_map_to_ollama_shape_with_num_predict() {
        let mut req = request(vec![Message::user("hi")]);
        req.options = ChatOptions {
            temperature: Some(0.5),
            max_tokens: Some(128),
            ..Default::default()
        };
        let body = chat_body(&req);
        assert_eq!(body["options"]["temperature"], 0.5);
        assert_eq!(body["options"]["num_predict"], 128);
        assert!(body["options"].get("top_p").is_none());
    }

    #[test]
    fn chat_response_parses_plain_reply() {
        let raw: RawChatResponse = serde_json::from_value(json!({
            "message": {"role": "assistant", "content": "hello there"},
            "done_reason": "stop",
            "prompt_eval_count": 10,
            "eval_count": 4
        }))
        .unwrap();
        let response = chat_response(raw).unwrap();
        assert_eq!(response.message.text(), "hello there");
        assert_eq!(response.stop_reason, StopReason::Stop);
        assert!(response.message.tool_calls.is_empty());
        assert_eq!(response.usage.prompt_tokens, 10);
        assert_eq!(response.usage.completion_tokens, 4);
    }

    #[test]
    fn chat_response_synthesizes_tool_call_ids() {
        let raw: RawChatResponse = serde_json::from_value(json!({
            "message": {
                "role": "assistant",
                "content": "",
                "tool_calls": [
                    {"function": {"name": "get_weather", "arguments": {"city": "Boston"}}},
                    {"function": {"name": "get_time", "arguments": {}}}
                ]
            }
        }))
        .unwrap();
        let response = chat_response(raw).unwrap();
        assert_eq!(response.stop_reason, StopReason::ToolCalls);
        assert_eq!(response.message.tool_calls[0].id, "call_0");
        assert_eq!(response.message.tool_calls[0].name, "get_weather");
        assert_eq!(response.message.tool_calls[0].arguments, json!({"city": "Boston"}));
        assert_eq!(response.message.tool_calls[1].id, "call_1");
    }

    #[test]
    fn chat_response_maps_length_done_reason() {
        let raw: RawChatResponse = serde_json::from_value(json!({
            "message": {"role": "assistant", "content": "cut off"},
            "done_reason": "length"
        }))
        .unwrap();
        let response = chat_response(raw).unwrap();
        assert_eq!(response.stop_reason, StopReason::Length);
    }

    #[test]
    fn chat_response_preserves_unknown_done_reason() {
        let raw: RawChatResponse = serde_json::from_value(json!({
            "message": {"role": "assistant", "content": "?"},
            "done_reason": "something_new"
        }))
        .unwrap();
        let response = chat_response(raw).unwrap();
        assert_eq!(response.stop_reason, StopReason::Other("something_new".to_owned()));
    }

    #[tokio::test]
    async fn chat_round_trips_through_http() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "message": {"role": "assistant", "content": "hi back"},
                "done_reason": "stop"
            })))
            .mount(&server)
            .await;

        let provider = OllamaProvider::new(server.uri(), Duration::from_secs(5));
        let response = provider.chat(&request(vec![Message::user("hi")])).await.unwrap();
        assert_eq!(response.message.text(), "hi back");
    }

    #[tokio::test]
    async fn http_error_status_is_reported_with_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(404).set_body_string("model not found"))
            .mount(&server)
            .await;

        let provider = OllamaProvider::new(server.uri(), Duration::from_secs(5));
        let err = provider
            .chat(&request(vec![Message::user("hi")]))
            .await
            .unwrap_err();
        match err {
            LlmError::Status { status, body, .. } => {
                assert_eq!(status, 404);
                assert_eq!(body, "model not found");
            }
            other => panic!("expected Status, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn malformed_response_body_is_a_decode_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .mount(&server)
            .await;

        let provider = OllamaProvider::new(server.uri(), Duration::from_secs(5));
        let err = provider
            .chat(&request(vec![Message::user("hi")]))
            .await
            .unwrap_err();
        assert!(matches!(err, LlmError::Decode { .. }));
    }
}
