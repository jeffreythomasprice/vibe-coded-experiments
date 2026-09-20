use std::time::Duration;

use async_trait::async_trait;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::llm::LlmError;
use crate::llm::message::{Content, Image, Message, Role};
use crate::llm::provider::{
    ChatOptions, ChatRequest, ChatResponse, Provider, StopReason, TokenChoice, TokenLogprob, Usage,
};
use crate::llm::tool::{ToolCall, ToolDef};

const PROVIDER_NAME: &str = "ollama";

const PHASE_TEMPLATE: &str = "{spinner} {msg} [{elapsed_precise}]";
const BLOB_TEMPLATE: &str =
    "{msg} [{elapsed_precise}] [{wide_bar}] {bytes}/{total_bytes} {bytes_per_sec} ({eta})";

pub struct OllamaProvider {
    client: reqwest::Client,
    pull_client: reqwest::Client,
    base_url: String,
}

impl OllamaProvider {
    pub fn new(base_url: impl Into<String>, timeout: Duration, pull_stall_timeout: Duration) -> Self {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .expect("reqwest client with a plain timeout is always buildable");
        let pull_client = reqwest::Client::builder()
            .read_timeout(pull_stall_timeout)
            .build()
            .expect("reqwest client with a plain read timeout is always buildable");
        Self {
            client,
            pull_client,
            base_url: base_url.into(),
        }
    }
}

#[async_trait]
impl Provider for OllamaProvider {
    fn name(&self) -> &'static str {
        PROVIDER_NAME
    }

    /// A model missing locally (HTTP 404 from `/api/chat`) is not fatal: it is
    /// pulled and the request retried once, so a valid model name works without
    /// the user having to run `ollama pull` themselves first.
    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, LlmError> {
        let original = match self.chat_once(request).await {
            Ok(response) => return Ok(response),
            Err(err) => err,
        };

        let LlmError::Status { status: 404, .. } = &original else {
            return Err(original);
        };

        tracing::debug!(
            target: "diffusion::llm",
            model = %request.model,
            "model missing locally, pulling before retrying chat"
        );

        if let Err(pull_err) = self.pull(&request.model).await {
            tracing::warn!(
                target: "diffusion::llm",
                model = %request.model,
                error = %pull_err,
                "pulling model failed"
            );
            return Err(original);
        }

        self.chat_once(request).await.map_err(|retry| {
            LlmError::PullRetryFailed {
                model: request.model.clone(),
                original: Box::new(original),
                retry: Box::new(retry),
            }
        })
    }
}

impl OllamaProvider {
    async fn chat_once(&self, request: &ChatRequest) -> Result<ChatResponse, LlmError> {
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

    /// Streams the pull so its progress can be shown: `/api/pull` emits one JSON
    /// object per line (manifest/verify/write phase markers, or a `digest`+`total`
    /// pair per blob with an absolute `completed` count), and a failure can arrive
    /// as an in-band `{"error": ...}` line after an HTTP 200, not just as a non-2xx
    /// status. The `pull_client`'s `read_timeout` (not a total deadline) covers both
    /// the initial `send()` and every `chunk()` read, so a pull that is still making
    /// progress is never killed, only one that goes silent.
    async fn pull(&self, model: &str) -> Result<(), LlmError> {
        let url = format!("{}/api/pull", self.base_url);
        let body = json!({ "model": model, "stream": true });

        let mut response = self
            .pull_client
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

        let multi = pull_progress();
        let phase = multi.add(
            ProgressBar::new_spinner()
                .with_style(ProgressStyle::with_template(PHASE_TEMPLATE).expect("static template is valid")),
        );
        phase.set_message(model.to_owned());
        phase.enable_steady_tick(Duration::from_millis(120));
        let blob_style = ProgressStyle::with_template(BLOB_TEMPLATE).expect("static template is valid");
        let mut bars: Vec<(String, ProgressBar)> = Vec::new();

        let mut buffer: Vec<u8> = Vec::new();
        let mut done = false;
        loop {
            let chunk = response.chunk().await.map_err(|source| LlmError::Request {
                url: url.clone(),
                source: Box::new(source),
            })?;
            let at_end = chunk.is_none();
            match chunk {
                Some(bytes) => buffer.extend_from_slice(&bytes),
                None => buffer.push(b'\n'),
            }

            for line in drain_lines(&mut buffer) {
                let raw: RawPullLine = serde_json::from_slice(&line).map_err(|source| LlmError::Decode {
                    provider: PROVIDER_NAME,
                    url: url.clone(),
                    source,
                })?;
                match pull_event(raw) {
                    PullEvent::Failed(message) => {
                        return Err(LlmError::PullFailed {
                            model: model.to_owned(),
                            message,
                        });
                    }
                    PullEvent::Done => {
                        done = true;
                        break;
                    }
                    PullEvent::Phase(status) => phase.set_message(format!("{model}: {status}")),
                    PullEvent::Layer {
                        digest,
                        total,
                        completed,
                    } => {
                        let index = match bars.iter().position(|(known, _)| *known == digest) {
                            Some(index) => index,
                            None => {
                                let bar = multi.add(ProgressBar::new(total).with_style(blob_style.clone()));
                                bar.set_message(short_digest(&digest));
                                bars.push((digest, bar));
                                bars.len() - 1
                            }
                        };
                        bars[index].1.set_position(completed);
                    }
                }
            }

            if done || at_end {
                break;
            }
        }

        if !done {
            return Err(LlmError::PullIncomplete {
                model: model.to_owned(),
            });
        }

        for (_, bar) in &bars {
            bar.finish();
        }
        phase.finish_and_clear();
        Ok(())
    }
}

#[cfg(not(test))]
fn pull_progress() -> MultiProgress {
    MultiProgress::new()
}

#[cfg(test)]
fn pull_progress() -> MultiProgress {
    MultiProgress::with_draw_target(indicatif::ProgressDrawTarget::hidden())
}

#[derive(Debug, Deserialize)]
struct RawPullLine {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    digest: Option<String>,
    #[serde(default)]
    total: Option<u64>,
    #[serde(default)]
    completed: Option<u64>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, PartialEq)]
enum PullEvent {
    Layer {
        digest: String,
        total: u64,
        completed: u64,
    },
    Phase(String),
    Done,
    Failed(String),
}

fn pull_event(raw: RawPullLine) -> PullEvent {
    if let Some(message) = raw.error {
        return PullEvent::Failed(message);
    }
    if raw.status.as_deref() == Some("success") {
        return PullEvent::Done;
    }
    if let (Some(digest), Some(total)) = (raw.digest, raw.total) {
        return PullEvent::Layer {
            digest,
            total,
            completed: raw.completed.unwrap_or(0),
        };
    }
    PullEvent::Phase(raw.status.unwrap_or_default())
}

/// Splits complete `\n`-terminated lines off the front of `buffer`, leaving any
/// trailing partial line (a chunk boundary rarely lands on a line boundary) for the
/// next call. Blank lines are dropped rather than parsed.
fn drain_lines(buffer: &mut Vec<u8>) -> Vec<Vec<u8>> {
    let mut lines = Vec::new();
    while let Some(index) = buffer.iter().position(|byte| *byte == b'\n') {
        let line: Vec<u8> = buffer.drain(..=index).collect();
        let trimmed = line.trim_ascii();
        if !trimmed.is_empty() {
            lines.push(trimmed.to_vec());
        }
    }
    lines
}

fn short_digest(digest: &str) -> String {
    digest.strip_prefix("sha256:").unwrap_or(digest).chars().take(12).collect()
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

    // logprobs/top_logprobs, think and format are top-level Ollama request keys,
    // unlike everything above (which nests under "options").
    if let Some(n) = request.options.logprobs {
        body["logprobs"] = json!(true);
        body["top_logprobs"] = json!(n);
    }
    if let Some(think) = request.options.think {
        body["think"] = json!(think);
    }
    if let Some(format) = &request.options.format {
        body["format"] = format.clone();
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
    #[serde(default)]
    logprobs: Vec<RawTokenChoice>,
}

#[derive(Debug, Deserialize)]
struct RawTokenChoice {
    token: String,
    logprob: f32,
    // Omitted from the response entirely when `top_logprobs` is 0, rather than
    // sent as an empty array.
    #[serde(default)]
    top_logprobs: Vec<RawTokenLogprob>,
}

#[derive(Debug, Deserialize)]
struct RawTokenLogprob {
    token: String,
    logprob: f32,
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

    let logprobs = raw
        .logprobs
        .into_iter()
        .map(|choice| TokenChoice {
            token: choice.token,
            logprob: choice.logprob,
            top: choice
                .top_logprobs
                .into_iter()
                .map(|top| TokenLogprob {
                    token: top.token,
                    logprob: top.logprob,
                })
                .collect(),
        })
        .collect();

    Ok(ChatResponse {
        message,
        stop_reason,
        usage: Usage {
            prompt_tokens: raw.prompt_eval_count,
            completion_tokens: raw.eval_count,
        },
        logprobs,
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
    fn logprobs_think_and_format_are_top_level_keys_not_nested_in_options() {
        let mut req = request(vec![Message::user("hi")]);
        req.options = ChatOptions {
            logprobs: Some(20),
            think: Some(false),
            format: Some(json!({"type": "object"})),
            ..Default::default()
        };
        let body = chat_body(&req);
        assert_eq!(body["logprobs"], true);
        assert_eq!(body["top_logprobs"], 20);
        assert_eq!(body["think"], false);
        assert_eq!(body["format"], json!({"type": "object"}));
        assert!(body["options"].get("logprobs").is_none());
    }

    #[test]
    fn absent_logprobs_think_format_omit_the_keys() {
        let body = chat_body(&request(vec![Message::user("hi")]));
        assert!(body.get("logprobs").is_none());
        assert!(body.get("top_logprobs").is_none());
        assert!(body.get("think").is_none());
        assert!(body.get("format").is_none());
    }

    #[test]
    fn chat_response_parses_logprobs() {
        let raw: RawChatResponse = serde_json::from_value(json!({
            "message": {"role": "assistant", "content": "Yes"},
            "done_reason": "length",
            "logprobs": [{
                "token": "Yes",
                "logprob": -0.08,
                "top_logprobs": [
                    {"token": "Yes", "logprob": -0.08},
                    {"token": "No", "logprob": -5.4}
                ]
            }]
        }))
        .unwrap();
        let response = chat_response(raw).unwrap();
        assert_eq!(response.stop_reason, StopReason::Length);
        assert_eq!(response.logprobs.len(), 1);
        assert_eq!(response.logprobs[0].token, "Yes");
        assert_eq!(response.logprobs[0].top.len(), 2);
        assert_eq!(response.logprobs[0].top[1].token, "No");
    }

    #[test]
    fn missing_top_logprobs_defaults_to_empty_rather_than_erroring() {
        let raw: RawChatResponse = serde_json::from_value(json!({
            "message": {"role": "assistant", "content": "Yes"},
            "logprobs": [{"token": "Yes", "logprob": -0.08}]
        }))
        .unwrap();
        let response = chat_response(raw).unwrap();
        assert!(response.logprobs[0].top.is_empty());
    }

    #[test]
    fn absent_logprobs_field_decodes_as_empty() {
        let raw: RawChatResponse = serde_json::from_value(json!({
            "message": {"role": "assistant", "content": "hi"}
        }))
        .unwrap();
        let response = chat_response(raw).unwrap();
        assert!(response.logprobs.is_empty());
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

        let provider = OllamaProvider::new(server.uri(), Duration::from_secs(5), Duration::from_secs(5));
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

        let provider = OllamaProvider::new(server.uri(), Duration::from_secs(5), Duration::from_secs(5));
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

        let provider = OllamaProvider::new(server.uri(), Duration::from_secs(5), Duration::from_secs(5));
        let err = provider
            .chat(&request(vec![Message::user("hi")]))
            .await
            .unwrap_err();
        assert!(matches!(err, LlmError::Decode { .. }));
    }

    #[tokio::test]
    async fn missing_model_is_pulled_and_the_chat_request_is_retried() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(404).set_body_string("model 'qwen3:4b' not found"))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "message": {"role": "assistant", "content": "hi back"},
                "done_reason": "stop"
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/pull"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"status": "success"})))
            .expect(1)
            .mount(&server)
            .await;

        let provider = OllamaProvider::new(server.uri(), Duration::from_secs(5), Duration::from_secs(5));
        let response = provider
            .chat(&request(vec![Message::user("hi")]))
            .await
            .unwrap();
        assert_eq!(response.message.text(), "hi back");
    }

    #[tokio::test]
    async fn pull_failure_surfaces_the_original_not_found_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(404).set_body_string("model 'qwen3:4b' not found"))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/pull"))
            .respond_with(
                ResponseTemplate::new(500)
                    .set_body_string("pull model manifest: file does not exist"),
            )
            .mount(&server)
            .await;

        let provider = OllamaProvider::new(server.uri(), Duration::from_secs(5), Duration::from_secs(5));
        let err = provider
            .chat(&request(vec![Message::user("hi")]))
            .await
            .unwrap_err();
        match err {
            LlmError::Status { status, body, .. } => {
                assert_eq!(status, 404);
                assert_eq!(body, "model 'qwen3:4b' not found");
            }
            other => panic!("expected the original chat Status error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn retry_failure_after_a_successful_pull_reports_both_errors() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(404).set_body_string("model 'qwen3:4b' not found"))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(500).set_body_string("out of memory"))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/pull"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"status": "success"})))
            .mount(&server)
            .await;

        let provider = OllamaProvider::new(server.uri(), Duration::from_secs(5), Duration::from_secs(5));
        let err = provider
            .chat(&request(vec![Message::user("hi")]))
            .await
            .unwrap_err();
        match err {
            LlmError::PullRetryFailed {
                model,
                original,
                retry,
            } => {
                assert_eq!(model, "qwen3:4b");
                assert!(matches!(*original, LlmError::Status { status: 404, .. }));
                assert!(matches!(*retry, LlmError::Status { status: 500, .. }));
            }
            other => panic!("expected PullRetryFailed, got {other:?}"),
        }
    }

    const PULL_SUCCESS_BODY: &str = concat!(
        "{\"status\":\"pulling manifest\"}\n",
        "{\"status\":\"pulling b0f58c4c1a3c\",\"digest\":\"sha256:b0f58c...\",\"total\":561}\n",
        "{\"status\":\"pulling b0f58c4c1a3c\",\"digest\":\"sha256:b0f58c...\",\"total\":561,\"completed\":561}\n",
        "{\"status\":\"verifying sha256 digest\"}\n",
        "{\"status\":\"writing manifest\"}\n",
        "{\"status\":\"success\"}\n",
    );

    fn parse_line(line: &[u8]) -> PullEvent {
        pull_event(serde_json::from_slice(line).unwrap())
    }

    #[test]
    fn drain_lines_leaves_a_partial_tail_in_the_buffer() {
        let mut buffer = b"{\"a\":1}\n{\"b\"".to_vec();
        let lines = drain_lines(&mut buffer);
        assert_eq!(lines, vec![b"{\"a\":1}".to_vec()]);
        assert_eq!(buffer, b"{\"b\"".to_vec());
    }

    #[test]
    fn drain_lines_drops_blank_lines_and_empties_a_buffer_ending_in_newline() {
        let mut buffer = b"\n{\"a\":1}\n\n".to_vec();
        let lines = drain_lines(&mut buffer);
        assert_eq!(lines, vec![b"{\"a\":1}".to_vec()]);
        assert!(buffer.is_empty());
    }

    #[test]
    fn chunk_boundaries_never_change_the_parsed_event_sequence() {
        fn events_for(body: &[u8], chunk_size: usize) -> Vec<PullEvent> {
            let mut buffer = Vec::new();
            let mut events = Vec::new();
            for chunk in body.chunks(chunk_size) {
                buffer.extend_from_slice(chunk);
                for line in drain_lines(&mut buffer) {
                    events.push(parse_line(&line));
                }
            }
            buffer.push(b'\n');
            for line in drain_lines(&mut buffer) {
                events.push(parse_line(&line));
            }
            events
        }

        let with_newline = PULL_SUCCESS_BODY.as_bytes();
        let without_newline = &with_newline[..with_newline.len() - 1];

        let expected = events_for(with_newline, with_newline.len());
        assert_eq!(*expected.last().unwrap(), PullEvent::Done);

        for size in 1..=with_newline.len() {
            assert_eq!(events_for(with_newline, size), expected, "chunk size {size}, with newline");
            assert_eq!(events_for(without_newline, size), expected, "chunk size {size}, without newline");
        }
    }

    #[test]
    fn pull_event_maps_a_blob_line_missing_completed_to_zero() {
        let event = parse_line(br#"{"status":"pulling abc","digest":"sha256:abc","total":561}"#);
        assert_eq!(
            event,
            PullEvent::Layer {
                digest: "sha256:abc".to_owned(),
                total: 561,
                completed: 0,
            }
        );
    }

    #[test]
    fn pull_event_maps_a_blob_line_with_completed() {
        let event = parse_line(br#"{"status":"pulling abc","digest":"sha256:abc","total":561,"completed":200}"#);
        assert_eq!(
            event,
            PullEvent::Layer {
                digest: "sha256:abc".to_owned(),
                total: 561,
                completed: 200,
            }
        );
    }

    #[test]
    fn pull_event_maps_a_phase_line() {
        assert_eq!(
            parse_line(br#"{"status":"pulling manifest"}"#),
            PullEvent::Phase("pulling manifest".to_owned())
        );
    }

    #[test]
    fn pull_event_maps_success() {
        assert_eq!(parse_line(br#"{"status":"success"}"#), PullEvent::Done);
    }

    #[test]
    fn pull_event_maps_an_in_band_error() {
        assert_eq!(
            parse_line(br#"{"error":"pull model manifest: file does not exist"}"#),
            PullEvent::Failed("pull model manifest: file does not exist".to_owned())
        );
    }

    #[test]
    fn pull_event_error_wins_over_status() {
        assert_eq!(
            parse_line(br#"{"status":"pulling manifest","error":"boom"}"#),
            PullEvent::Failed("boom".to_owned())
        );
    }

    #[test]
    fn short_digest_strips_prefix_and_truncates() {
        assert_eq!(short_digest("sha256:b0f58c4c1a3ca56f34a76"), "b0f58c4c1a3c");
    }

    #[test]
    fn short_digest_truncates_a_digest_without_the_prefix() {
        assert_eq!(short_digest("b0f58c4c1a3ca56f34a76"), "b0f58c4c1a3c");
    }

    #[test]
    fn progress_templates_are_valid() {
        assert!(ProgressStyle::with_template(PHASE_TEMPLATE).is_ok());
        assert!(ProgressStyle::with_template(BLOB_TEMPLATE).is_ok());
    }

    #[tokio::test]
    async fn pull_succeeds_on_a_full_streaming_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/pull"))
            .respond_with(ResponseTemplate::new(200).set_body_string(PULL_SUCCESS_BODY))
            .mount(&server)
            .await;

        let provider = OllamaProvider::new(server.uri(), Duration::from_secs(5), Duration::from_secs(5));
        provider.pull("qwen3:4b").await.unwrap();
    }

    #[tokio::test]
    async fn pull_reports_an_in_band_failure() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/pull"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                "{\"status\":\"pulling manifest\"}\n{\"error\":\"pull model manifest: file does not exist\"}\n",
            ))
            .mount(&server)
            .await;

        let provider = OllamaProvider::new(server.uri(), Duration::from_secs(5), Duration::from_secs(5));
        let err = provider.pull("qwen3:4b").await.unwrap_err();
        match err {
            LlmError::PullFailed { model, message } => {
                assert_eq!(model, "qwen3:4b");
                assert_eq!(message, "pull model manifest: file does not exist");
            }
            other => panic!("expected PullFailed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn pull_reports_incomplete_when_the_stream_ends_without_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/pull"))
            .respond_with(ResponseTemplate::new(200).set_body_string("{\"status\":\"pulling manifest\"}\n"))
            .mount(&server)
            .await;

        let provider = OllamaProvider::new(server.uri(), Duration::from_secs(5), Duration::from_secs(5));
        let err = provider.pull("qwen3:4b").await.unwrap_err();
        assert!(matches!(err, LlmError::PullIncomplete { model } if model == "qwen3:4b"));
    }

    #[tokio::test]
    async fn pull_stall_timeout_fires_when_the_server_goes_silent() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/pull"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(PULL_SUCCESS_BODY)
                    .set_delay(Duration::from_millis(200)),
            )
            .mount(&server)
            .await;

        let provider = OllamaProvider::new(server.uri(), Duration::from_secs(5), Duration::from_millis(20));
        let err = provider.pull("qwen3:4b").await.unwrap_err();
        match err {
            LlmError::Request { source, .. } => assert!(source.is_timeout()),
            other => panic!("expected a timed-out Request error, got {other:?}"),
        }
    }
}
