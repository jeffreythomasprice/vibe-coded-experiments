//! OpenAI Responses API (`POST {base_url}/v1/responses`) request/response
//! shapes and pure conversions to/from `shared::llm` types.
//!
//! Every function here is pure — no I/O — so it's unit-tested directly on
//! recorded fixtures under `fixtures/`. See `openai::mod` for the thin
//! transport layer that calls these, and `openai::images` for the separate
//! `/v1/images/generations` endpoint this provider also implements.

use serde::Deserialize;

use shared::llm::{
    ChatOptions, CompletedMessage, ContentBlock, Conversation, Delta, Effort, MediaType, Message,
    ModelDetails, ModelInfo, Role, StopReason, StreamEvent, Thinking, ToolChoice, ToolDef,
    ToolResultContent, Usage,
};

use crate::llm::error::{ApiErrorKind, LlmError};
use crate::llm::sse::{SseDecoder, SseFrame};

pub const PROVIDER: &str = "openai";

// ---------------------------------------------------------------------
// Request building
// ---------------------------------------------------------------------

/// Build the JSON body for `POST {base_url}/v1/responses`. Model and
/// `max_tokens` come from `options` — both required fields on
/// `ChatOptions`, since neither has a sensible config-level default (see
/// `crate::llm::config`'s module doc).
pub fn build_request(
    conversation: &Conversation,
    options: &ChatOptions,
    stream: bool,
) -> serde_json::Value {
    let mut body = serde_json::json!({
        "model": options.model,
        "input": to_wire_input(&conversation.messages),
        "stream": stream,
        "max_output_tokens": options.max_tokens,
    });

    if let Some(instructions) = &conversation.system {
        body["instructions"] = serde_json::json!(instructions);
    }
    if !options.tools.is_empty() {
        body["tools"] =
            serde_json::json!(options.tools.iter().map(to_wire_tool).collect::<Vec<_>>());
    }
    if let Some(choice) = &options.tool_choice {
        body["tool_choice"] = to_wire_tool_choice(choice);
    }
    if !options.stop_sequences.is_empty() {
        body["stop"] = serde_json::json!(options.stop_sequences);
    }
    if let Thinking::Adaptive { effort } = &options.thinking {
        body["reasoning"] = serde_json::json!({"effort": effort_str(*effort)});
    }

    body
}

fn effort_str(effort: Effort) -> &'static str {
    match effort {
        Effort::Low => "low",
        Effort::Medium => "medium",
        Effort::High => "high",
    }
}

/// Tool definitions are internally tagged in the Responses API
/// (`{"type":"function","name":...}`), unlike Chat Completions' externally
/// tagged `{"type":"function","function":{"name":...}}` — easy to get wrong
/// from muscle memory.
fn to_wire_tool(tool: &ToolDef) -> serde_json::Value {
    serde_json::json!({
        "type": "function",
        "name": tool.name,
        "description": tool.description,
        "parameters": tool.input_schema,
    })
}

fn to_wire_tool_choice(choice: &ToolChoice) -> serde_json::Value {
    match choice {
        ToolChoice::Auto => serde_json::json!("auto"),
        ToolChoice::Any => serde_json::json!("required"),
        ToolChoice::None => serde_json::json!("none"),
        ToolChoice::Tool { name } => serde_json::json!({"type": "function", "name": name}),
    }
}

fn role_str(role: Role) -> &'static str {
    match role {
        Role::User => "user",
        Role::Assistant => "assistant",
    }
}

fn media_type_str(mt: &MediaType) -> String {
    match mt {
        MediaType::Png => "image/png".to_string(),
        MediaType::Jpeg => "image/jpeg".to_string(),
        MediaType::Webp => "image/webp".to_string(),
        MediaType::Gif => "image/gif".to_string(),
        MediaType::Other(s) => s.clone(),
    }
}

/// Flatten every `shared::llm::Message` into the Responses API's `input`
/// item array. Unlike Ollama, text and image content parts stay in one
/// message item (no lossy flattening there) — only `tool_use`/`tool_result`
/// blocks become their own top-level items, matching the Responses API's own
/// `function_call`/`function_call_output` item types.
fn to_wire_input(messages: &[Message]) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    for message in messages {
        let mut parts: Vec<serde_json::Value> = Vec::new();
        for block in &message.content {
            match block {
                ContentBlock::Text { text } => {
                    // A prior assistant turn being replayed as input uses
                    // `output_text` (what the model actually produced); a
                    // user turn uses `input_text`.
                    let part_type = match message.role {
                        Role::User => "input_text",
                        Role::Assistant => "output_text",
                    };
                    parts.push(serde_json::json!({"type": part_type, "text": text}));
                }
                ContentBlock::Image { source } => {
                    if message.role == Role::Assistant {
                        tracing::warn!(
                            "dropping an Image block from an assistant message: the Responses \
                             API has no matching output content type"
                        );
                        continue;
                    }
                    parts.push(serde_json::json!({
                        "type": "input_image",
                        "image_url": format!(
                            "data:{};base64,{}",
                            media_type_str(&source.media_type),
                            source.data
                        ),
                    }));
                }
                ContentBlock::ToolUse { id, name, input } => {
                    flush_input_message(&mut parts, &mut out, message.role);
                    out.push(serde_json::json!({
                        "type": "function_call",
                        "call_id": id,
                        "name": name,
                        "arguments": input.to_string(),
                    }));
                }
                ContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    is_error,
                } => {
                    flush_input_message(&mut parts, &mut out, message.role);
                    let mut text = String::new();
                    for c in content {
                        match c {
                            ToolResultContent::Text { text: t } => {
                                if !text.is_empty() {
                                    text.push('\n');
                                }
                                text.push_str(t);
                            }
                            ToolResultContent::Image { .. } => {
                                tracing::warn!(
                                    "dropping an image inside a tool_result: not supported for \
                                     the OpenAI provider yet"
                                );
                            }
                        }
                    }
                    if *is_error && !text.is_empty() {
                        text = format!("Error: {text}");
                    }
                    out.push(serde_json::json!({
                        "type": "function_call_output",
                        "call_id": tool_use_id,
                        "output": text,
                    }));
                }
                ContentBlock::Thinking { .. } | ContentBlock::RedactedThinking { .. } => {
                    tracing::warn!(
                        "dropping a thinking block: not supported when replaying history to \
                         the OpenAI provider"
                    );
                }
                ContentBlock::Unknown => {
                    tracing::warn!(
                        "dropping an Unknown content block: its payload did not survive deserialization"
                    );
                }
            }
        }
        flush_input_message(&mut parts, &mut out, message.role);
    }
    out
}

fn flush_input_message(parts: &mut Vec<serde_json::Value>, out: &mut Vec<serde_json::Value>, role: Role) {
    if parts.is_empty() {
        return;
    }
    out.push(serde_json::json!({
        "role": role_str(role),
        "content": std::mem::take(parts),
    }));
}

// ---------------------------------------------------------------------
// Non-streaming response parsing
// ---------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct WireResponse {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    incomplete_details: Option<WireIncompleteDetails>,
    #[serde(default)]
    output: Vec<WireOutputItem>,
    #[serde(default)]
    usage: Option<WireUsage>,
}

#[derive(Debug, Deserialize)]
struct WireIncompleteDetails {
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WireOutputItem {
    Message {
        #[serde(default)]
        content: Vec<WireContentPart>,
    },
    FunctionCall {
        call_id: String,
        name: String,
        #[serde(default)]
        arguments: String,
    },
    /// A summarized-reasoning item. Best-effort support: mapped to a single
    /// `Thinking` block with no signature (the Responses API has no
    /// Anthropic-style signature to preserve).
    Reasoning {
        #[serde(default)]
        summary: Vec<WireSummaryPart>,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WireContentPart {
    OutputText { text: String },
    Refusal { refusal: String },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WireSummaryPart {
    SummaryText { text: String },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Default, Deserialize)]
struct WireUsage {
    #[serde(default)]
    input_tokens: u32,
    #[serde(default)]
    output_tokens: u32,
    #[serde(default)]
    input_tokens_details: Option<WireInputTokensDetails>,
}

#[derive(Debug, Deserialize)]
struct WireInputTokensDetails {
    #[serde(default)]
    cached_tokens: Option<u32>,
}

pub fn parse_response(body: &str) -> Result<CompletedMessage, LlmError> {
    let wire: WireResponse = serde_json::from_str(body).map_err(|source| LlmError::Decode {
        provider: PROVIDER,
        context: "responses response".to_string(),
        source,
    })?;

    let mut content = Vec::new();
    let mut saw_tool_use = false;
    for item in wire.output {
        match item {
            WireOutputItem::Message { content: parts } => {
                for part in parts {
                    match part {
                        WireContentPart::OutputText { text } => {
                            content.push(ContentBlock::Text { text })
                        }
                        WireContentPart::Refusal { refusal } => {
                            content.push(ContentBlock::Text { text: refusal })
                        }
                        WireContentPart::Unknown => {
                            tracing::warn!("ignoring an unrecognized Responses content part type");
                        }
                    }
                }
            }
            WireOutputItem::FunctionCall {
                call_id,
                name,
                arguments,
            } => {
                saw_tool_use = true;
                let input = parse_arguments(&call_id, &arguments)?;
                content.push(ContentBlock::ToolUse {
                    id: call_id,
                    name,
                    input,
                });
            }
            WireOutputItem::Reasoning { summary } => {
                let thinking: String = summary
                    .into_iter()
                    .filter_map(|p| match p {
                        WireSummaryPart::SummaryText { text } => Some(text),
                        WireSummaryPart::Unknown => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                if !thinking.is_empty() {
                    content.push(ContentBlock::Thinking {
                        thinking,
                        signature: None,
                    });
                }
            }
            WireOutputItem::Unknown => {
                tracing::warn!("ignoring an unrecognized Responses output item type");
            }
        }
    }

    let stop_reason = if saw_tool_use {
        StopReason::ToolUse
    } else {
        status_to_stop_reason(wire.status.as_deref(), wire.incomplete_details)
    };

    let usage = wire.usage.unwrap_or_default();
    Ok(CompletedMessage {
        role: Role::Assistant,
        content,
        stop_reason,
        usage: to_shared_usage(usage),
        model: wire.model,
    })
}

fn parse_arguments(call_id: &str, arguments: &str) -> Result<serde_json::Value, LlmError> {
    if arguments.trim().is_empty() {
        return Ok(serde_json::json!({}));
    }
    serde_json::from_str(arguments).map_err(|source| LlmError::Decode {
        provider: PROVIDER,
        context: format!("function_call arguments for {call_id}"),
        source,
    })
}

fn status_to_stop_reason(
    status: Option<&str>,
    incomplete_details: Option<WireIncompleteDetails>,
) -> StopReason {
    match status {
        Some("completed") | None => StopReason::EndTurn,
        Some("incomplete") => match incomplete_details.and_then(|d| d.reason) {
            Some(reason) if reason == "max_output_tokens" => StopReason::MaxTokens,
            Some(reason) => StopReason::Other(reason),
            None => StopReason::Other("incomplete".to_string()),
        },
        Some(other) => StopReason::Other(other.to_string()),
    }
}

fn to_shared_usage(usage: WireUsage) -> Usage {
    Usage {
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        cache_read_input_tokens: usage.input_tokens_details.and_then(|d| d.cached_tokens),
        cache_creation_input_tokens: None,
    }
}

// ---------------------------------------------------------------------
// Error parsing
// ---------------------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
struct WireErrorEnvelope {
    #[serde(default)]
    error: Option<WireErrorDetail>,
}

#[derive(Debug, Default, Deserialize)]
struct WireErrorDetail {
    #[serde(default)]
    message: String,
}

pub fn parse_error(status: u16, body: &str) -> LlmError {
    let kind = ApiErrorKind::from_status(status);
    let envelope: WireErrorEnvelope = serde_json::from_str(body).unwrap_or_default();
    let message = envelope
        .error
        .map(|e| e.message)
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| body.to_string());
    LlmError::Status {
        provider: PROVIDER,
        status,
        kind,
        message,
        request_id: None,
    }
}

fn extract_stream_error_message(data: &str) -> String {
    let envelope: WireErrorEnvelope = serde_json::from_str(data).unwrap_or_default();
    envelope
        .error
        .map(|e| e.message)
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| data.to_string())
}

// ---------------------------------------------------------------------
// Model listing
// ---------------------------------------------------------------------

/// `GET {base_url}/v1/models` — unlike Anthropic, unpaginated: one call
/// returns every model the API key can use.
#[derive(Debug, Deserialize)]
struct WireModelsResponse {
    #[serde(default)]
    data: Vec<WireModel>,
}

#[derive(Debug, Deserialize)]
struct WireModel {
    id: String,
    /// Already unix seconds on the wire, unlike Anthropic's RFC 3339
    /// `created_at` — no conversion needed here.
    #[serde(default)]
    created: Option<i64>,
    #[serde(default)]
    owned_by: Option<String>,
}

pub fn parse_models(body: &str) -> Result<Vec<ModelInfo>, LlmError> {
    let wire: WireModelsResponse = serde_json::from_str(body).map_err(|source| LlmError::Decode {
        provider: PROVIDER,
        context: "models response".to_string(),
        source,
    })?;

    Ok(wire
        .data
        .into_iter()
        .map(|m| ModelInfo {
            id: m.id.clone(),
            display_name: None,
            created_at: m.created,
            details: classify(&m.id, m.owned_by.unwrap_or_default()),
        })
        .collect())
}

/// Sort a model id into chat / embedding / image, the only signal
/// `GET /v1/models` gives us — the response carries no capability field, so
/// this is an id-prefix rule rather than something read off the wire.
///
/// Deliberately reports no dimensions or input-token limit for an embedding
/// model: OpenAI's API never states either, and a hardcoded table of them
/// would go stale silently. See `ModelDetails::OpenAiEmbedding`'s doc.
fn classify(id: &str, owned_by: String) -> ModelDetails {
    if id.starts_with("text-embedding-") {
        ModelDetails::OpenAiEmbedding { owned_by }
    } else if id.starts_with("gpt-image-") || id.starts_with("dall-e") {
        ModelDetails::OpenAiImage { owned_by }
    } else {
        ModelDetails::OpenAi { owned_by }
    }
}

// ---------------------------------------------------------------------
// Streaming
// ---------------------------------------------------------------------

/// State a live SSE stream carries across frames. Unlike Anthropic, block
/// indices don't need to be allocated here — the Responses API's own
/// `output_index` on every event is used directly as the shared block index.
#[derive(Debug, Default)]
pub struct StreamState {
    started: bool,
    saw_tool_use: bool,
}

#[derive(Debug, Deserialize)]
struct WireCreated {
    response: WireCreatedResponse,
}

#[derive(Debug, Deserialize)]
struct WireCreatedResponse {
    #[serde(default)]
    model: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WireOutputItemAdded {
    output_index: usize,
    item: WireStreamItem,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WireStreamItem {
    Message {},
    FunctionCall {
        #[serde(default)]
        call_id: String,
        #[serde(default)]
        name: String,
    },
    Reasoning {},
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize)]
struct WireTextDelta {
    output_index: usize,
    delta: String,
}

#[derive(Debug, Deserialize)]
struct WireFunctionCallArgsDelta {
    output_index: usize,
    delta: String,
}

#[derive(Debug, Deserialize)]
struct WireOutputItemDone {
    output_index: usize,
}

#[derive(Debug, Deserialize)]
struct WireCompleted {
    response: WireCompletedResponse,
}

#[derive(Debug, Deserialize)]
struct WireCompletedResponse {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    incomplete_details: Option<WireIncompleteDetails>,
    #[serde(default)]
    usage: Option<WireUsage>,
}

fn decode_error(context: &str, source: serde_json::Error) -> LlmError {
    LlmError::Decode {
        provider: PROVIDER,
        context: context.to_string(),
        source,
    }
}

/// Translate one already-decoded SSE frame into zero-or-one `StreamEvent`s.
/// Reasoning-item streaming beyond a bare `BlockStart`/`BlockStop` isn't
/// modeled — the delta event name for summarized reasoning text isn't
/// pinned down with the same confidence as the text/tool-call path, and an
/// unrecognized event is always safely ignored rather than guessed at.
pub fn translate_frame(
    frame: &SseFrame,
    state: &mut StreamState,
) -> Result<Option<StreamEvent>, LlmError> {
    match frame.event.as_str() {
        "response.created" => {
            if state.started {
                return Ok(None);
            }
            state.started = true;
            let wire: WireCreated = serde_json::from_str(&frame.data)
                .map_err(|e| decode_error("response.created event", e))?;
            Ok(Some(StreamEvent::MessageStart {
                model: wire.response.model.unwrap_or_default(),
            }))
        }
        "response.output_item.added" => {
            let wire: WireOutputItemAdded = serde_json::from_str(&frame.data)
                .map_err(|e| decode_error("response.output_item.added event", e))?;
            let block = match wire.item {
                WireStreamItem::Message {} => Some(ContentBlock::Text {
                    text: String::new(),
                }),
                WireStreamItem::FunctionCall { call_id, name } => {
                    state.saw_tool_use = true;
                    Some(ContentBlock::ToolUse {
                        id: call_id,
                        name,
                        input: serde_json::Value::Null,
                    })
                }
                WireStreamItem::Reasoning {} => Some(ContentBlock::Thinking {
                    thinking: String::new(),
                    signature: None,
                }),
                WireStreamItem::Unknown => {
                    tracing::warn!("ignoring an unrecognized Responses output item type in a stream");
                    None
                }
            };
            Ok(block.map(|block| StreamEvent::BlockStart {
                index: wire.output_index,
                block,
            }))
        }
        "response.output_text.delta" => {
            let wire: WireTextDelta = serde_json::from_str(&frame.data)
                .map_err(|e| decode_error("response.output_text.delta event", e))?;
            Ok(Some(StreamEvent::Delta {
                index: wire.output_index,
                delta: Delta::Text { text: wire.delta },
            }))
        }
        "response.function_call_arguments.delta" => {
            let wire: WireFunctionCallArgsDelta = serde_json::from_str(&frame.data)
                .map_err(|e| decode_error("response.function_call_arguments.delta event", e))?;
            Ok(Some(StreamEvent::Delta {
                index: wire.output_index,
                delta: Delta::ToolInputJson {
                    partial_json: wire.delta,
                },
            }))
        }
        "response.output_item.done" => {
            let wire: WireOutputItemDone = serde_json::from_str(&frame.data)
                .map_err(|e| decode_error("response.output_item.done event", e))?;
            Ok(Some(StreamEvent::BlockStop {
                index: wire.output_index,
            }))
        }
        "response.completed" => {
            let wire: WireCompleted = serde_json::from_str(&frame.data)
                .map_err(|e| decode_error("response.completed event", e))?;
            let stop_reason = if state.saw_tool_use {
                StopReason::ToolUse
            } else {
                status_to_stop_reason(
                    wire.response.status.as_deref(),
                    wire.response.incomplete_details,
                )
            };
            Ok(Some(StreamEvent::MessageStop {
                stop_reason,
                usage: to_shared_usage(wire.response.usage.unwrap_or_default()),
            }))
        }
        "response.error" | "error" => Err(LlmError::Stream {
            provider: PROVIDER,
            message: extract_stream_error_message(&frame.data),
        }),
        other => {
            tracing::warn!(event = other, "ignoring an unrecognized OpenAI Responses SSE event");
            Ok(None)
        }
    }
}

/// Decode a complete SSE transcript (e.g. a recorded fixture) into its
/// `StreamEvent`s in one pass — the counterpart of
/// `anthropic::wire::parse_stream_events`.
pub fn parse_stream_events(sse_text: &str) -> Result<Vec<StreamEvent>, LlmError> {
    let mut decoder = SseDecoder::new();
    let frames = decoder.push(sse_text.as_bytes());
    let mut state = StreamState::default();
    let mut events = Vec::with_capacity(frames.len());
    for frame in &frames {
        if let Some(event) = translate_frame(frame, &mut state)? {
            events.push(event);
        }
    }
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::accumulate::MessageAccumulator;

    const RESPONSE_TEXT: &str = include_str!("fixtures/response_text.json");
    const RESPONSE_TOOL_CALL: &str = include_str!("fixtures/response_tool_call.json");
    const STREAM_TEXT: &str = include_str!("fixtures/stream_text.sse");
    const ERROR: &str = include_str!("fixtures/error.json");
    const MODELS: &str = include_str!("fixtures/models.json");

    /// Only used to check that a request's model/max_tokens land on the
    /// wire unchanged — not a config default.
    const MODEL: &str = "gpt-5.6";

    #[test]
    fn builds_a_responses_request_with_internally_tagged_tools() {
        let tool = ToolDef {
            name: "get_weather".to_string(),
            description: "Get the weather".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        };
        let conv = Conversation {
            system: Some("Be terse.".to_string()),
            messages: vec![Message::user(vec![ContentBlock::Text {
                text: "Hi".to_string(),
            }])],
        };
        let options = ChatOptions {
            tools: vec![tool],
            ..ChatOptions::new(MODEL, 1024)
        };
        let body = build_request(&conv, &options, false);
        assert_eq!(body["model"], serde_json::json!("gpt-5.6"));
        assert_eq!(body["instructions"], serde_json::json!("Be terse."));
        assert_eq!(
            body["input"][0],
            serde_json::json!({"role": "user", "content": [{"type": "input_text", "text": "Hi"}]})
        );
        // Internally tagged: no nested "function" object, unlike Chat Completions.
        assert_eq!(body["tools"][0]["type"], serde_json::json!("function"));
        assert_eq!(body["tools"][0]["name"], serde_json::json!("get_weather"));
        assert!(body["tools"][0].get("function").is_none());
    }

    /// `max_tokens` is a required field on `ChatOptions` (see
    /// `crate::llm::config`'s module doc), so `max_output_tokens` must always
    /// land on the wire — never omitted the way it used to be when the field
    /// was optional.
    #[test]
    fn max_output_tokens_is_always_sent() {
        let body = build_request(&Conversation::default(), &ChatOptions::new(MODEL, 512), false);
        assert_eq!(body["max_output_tokens"], serde_json::json!(512));
    }

    #[test]
    fn encodes_images_as_data_uris() {
        let conv = Conversation {
            system: None,
            messages: vec![Message::user(vec![ContentBlock::Image {
                source: shared::llm::ImageSource {
                    media_type: MediaType::Png,
                    data: "aGVsbG8=".to_string(),
                },
            }])],
        };
        let body = build_request(&conv, &ChatOptions::new(MODEL, 1024), false);
        assert_eq!(
            body["input"][0]["content"][0]["image_url"],
            serde_json::json!("data:image/png;base64,aGVsbG8=")
        );
    }

    #[test]
    fn replayed_assistant_text_uses_output_text_not_input_text() {
        let conv = Conversation {
            system: None,
            messages: vec![Message::assistant(vec![ContentBlock::Text {
                text: "Paris.".to_string(),
            }])],
        };
        let body = build_request(&conv, &ChatOptions::new(MODEL, 1024), false);
        assert_eq!(
            body["input"][0]["content"][0]["type"],
            serde_json::json!("output_text")
        );
    }

    #[test]
    fn tool_use_and_tool_result_become_separate_top_level_items() {
        let conv = Conversation {
            system: None,
            messages: vec![
                Message::assistant(vec![ContentBlock::ToolUse {
                    id: "call_1".to_string(),
                    name: "get_weather".to_string(),
                    input: serde_json::json!({"location": "Paris"}),
                }]),
                Message::user(vec![ContentBlock::ToolResult {
                    tool_use_id: "call_1".to_string(),
                    content: vec![ToolResultContent::Text {
                        text: "72F and sunny".to_string(),
                    }],
                    is_error: false,
                }]),
            ],
        };
        let body = build_request(&conv, &ChatOptions::new(MODEL, 1024), false);
        let input = body["input"].as_array().unwrap();
        assert_eq!(input.len(), 2);
        assert_eq!(input[0]["type"], serde_json::json!("function_call"));
        assert_eq!(input[0]["call_id"], serde_json::json!("call_1"));
        assert_eq!(input[1]["type"], serde_json::json!("function_call_output"));
        assert_eq!(input[1]["output"], serde_json::json!("72F and sunny"));
    }

    #[test]
    fn parses_message_and_function_call_items() {
        let message = parse_response(RESPONSE_TOOL_CALL).unwrap();
        assert_eq!(message.stop_reason, StopReason::ToolUse);
        assert_eq!(message.content.len(), 2);
        match &message.content[1] {
            ContentBlock::ToolUse { id, name, input } => {
                assert_eq!(id, "call_01XYZ");
                assert_eq!(name, "get_weather");
                assert_eq!(input, &serde_json::json!({"location": "Paris"}));
            }
            other => panic!("expected ToolUse, got {other:?}"),
        }
    }

    #[test]
    fn correlates_tool_results_by_call_id() {
        // The parsed ToolUse's id is exactly the call_id from the response —
        // that id is what a caller must echo back in a function_call_output
        // item's call_id.
        let message = parse_response(RESPONSE_TOOL_CALL).unwrap();
        let ContentBlock::ToolUse { id, .. } = &message.content[1] else {
            panic!("expected ToolUse");
        };
        let conv = Conversation {
            system: None,
            messages: vec![
                Message::assistant(message.content.clone()),
                Message::user(vec![ContentBlock::ToolResult {
                    tool_use_id: id.clone(),
                    content: vec![ToolResultContent::Text {
                        text: "sunny".to_string(),
                    }],
                    is_error: false,
                }]),
            ],
        };
        let body = build_request(&conv, &ChatOptions::new(MODEL, 1024), false);
        assert_eq!(body["input"][1]["call_id"], serde_json::json!(id));
    }

    #[test]
    fn parses_a_text_only_response() {
        let message = parse_response(RESPONSE_TEXT).unwrap();
        assert_eq!(message.stop_reason, StopReason::EndTurn);
        assert_eq!(message.model.as_deref(), Some("gpt-5.6"));
        assert_eq!(message.usage.input_tokens, 10);
        assert_eq!(message.usage.output_tokens, 8);
        match &message.content[0] {
            ContentBlock::Text { text } => assert_eq!(text, "The capital of France is Paris."),
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn maps_openai_error_envelopes_to_typed_errors() {
        let err = parse_error(400, ERROR);
        match err {
            LlmError::Status { kind, message, .. } => {
                assert_eq!(kind, ApiErrorKind::InvalidRequest);
                assert_eq!(message, "Invalid value for 'model'.");
            }
            other => panic!("expected Status, got {other:?}"),
        }
    }

    #[test]
    fn parses_semantic_sse_events_into_deltas() {
        let events = parse_stream_events(STREAM_TEXT).unwrap();
        assert!(matches!(events[0], StreamEvent::MessageStart { .. }));
        let text: String = events
            .iter()
            .filter_map(|e| match e {
                StreamEvent::Delta {
                    delta: Delta::Text { text },
                    ..
                } => Some(text.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(text, "The capital of France is Paris.");
        assert!(matches!(events.last(), Some(StreamEvent::MessageStop { .. })));
    }

    #[test]
    fn streamed_fixture_accumulates_to_the_same_message_as_the_blocking_fixture() {
        let events = parse_stream_events(STREAM_TEXT).unwrap();
        let mut acc = MessageAccumulator::new();
        for event in events {
            acc.push(event).unwrap();
        }
        let streamed = acc.finish().unwrap();
        let blocking = parse_response(RESPONSE_TEXT).unwrap();

        assert_eq!(streamed.stop_reason, blocking.stop_reason);
        assert_eq!(streamed.usage.input_tokens, blocking.usage.input_tokens);
        assert_eq!(streamed.usage.output_tokens, blocking.usage.output_tokens);
        assert_eq!(
            serde_json::to_value(&streamed.content).unwrap(),
            serde_json::to_value(&blocking.content).unwrap()
        );
    }

    #[test]
    fn parses_a_models_list() {
        let models = parse_models(MODELS).unwrap();
        assert_eq!(models.len(), 3);
        assert_eq!(models[0].id, "gpt-5.6");
        assert_eq!(models[0].created_at, Some(1_753_315_200));
        match &models[0].details {
            ModelDetails::OpenAi { owned_by } => assert_eq!(owned_by, "system"),
            other => panic!("expected OpenAi details, got {other:?}"),
        }
    }

    #[test]
    fn classifies_chat_image_and_embedding_models_by_id_prefix() {
        let models = parse_models(MODELS).unwrap();

        let chat = models.iter().find(|m| m.id == "gpt-5.6").unwrap();
        assert!(matches!(chat.details, ModelDetails::OpenAi { .. }));
        assert!(!chat.details.is_embedding());
        assert!(!chat.details.is_image());

        let image = models.iter().find(|m| m.id == "gpt-image-2").unwrap();
        assert!(matches!(image.details, ModelDetails::OpenAiImage { .. }));
        assert!(image.details.is_image());
        assert!(!image.details.is_embedding());

        let embedding = models
            .iter()
            .find(|m| m.id == "text-embedding-3-small")
            .unwrap();
        assert!(matches!(
            embedding.details,
            ModelDetails::OpenAiEmbedding { .. }
        ));
        assert!(embedding.details.is_embedding());
        // OpenAI never reports dimensions or an input limit for any model.
        assert_eq!(embedding.details.embedding_dimensions(), None);
        assert_eq!(embedding.details.max_input_tokens(), None);
    }

    #[test]
    fn classify_also_recognizes_dall_e() {
        assert!(matches!(
            classify("dall-e-3", "system".to_string()),
            ModelDetails::OpenAiImage { .. }
        ));
    }
}
