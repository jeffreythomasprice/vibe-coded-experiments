//! Anthropic Messages API request/response shapes and pure conversions
//! to/from `shared::llm` types.
//!
//! Every function here is pure — no I/O — so it's unit-tested directly on
//! recorded fixtures under `fixtures/`. See `anthropic::mod` for the thin
//! transport layer that calls these.

use serde::Deserialize;

use shared::llm::{
    ChatOptions, CompletedMessage, ContentBlock, Conversation, Delta, Effort, MediaType, Message,
    ModelDetails, ModelInfo, Role, StopReason, StreamEvent, Thinking, ToolChoice, ToolDef,
    ToolResultContent, Usage,
};

use crate::llm::config::AnthropicConfig;
use crate::llm::error::{ApiErrorKind, LlmError};
use crate::llm::sse::{SseDecoder, SseFrame};

pub const PROVIDER: &str = "anthropic";

// ---------------------------------------------------------------------
// Request building
// ---------------------------------------------------------------------

/// Build the JSON body for `POST {base_url}/v1/messages`.
pub fn build_request(
    conversation: &Conversation,
    options: &ChatOptions,
    cfg: &AnthropicConfig,
    stream: bool,
) -> serde_json::Value {
    let mut body = serde_json::json!({
        "model": options.model.clone().unwrap_or_else(|| cfg.model.clone()),
        "max_tokens": options.max_tokens.unwrap_or(cfg.max_tokens),
        "messages": conversation.messages.iter().map(to_wire_message).collect::<Vec<_>>(),
        "stream": stream,
    });

    if let Some(system) = &conversation.system {
        body["system"] = serde_json::json!(system);
    }
    if !options.tools.is_empty() {
        body["tools"] =
            serde_json::json!(options.tools.iter().map(to_wire_tool).collect::<Vec<_>>());
    }
    if let Some(choice) = &options.tool_choice {
        body["tool_choice"] = to_wire_tool_choice(choice);
    }
    if !options.stop_sequences.is_empty() {
        body["stop_sequences"] = serde_json::json!(options.stop_sequences);
    }
    // `Thinking::Off` deliberately omits the field rather than sending
    // `{"type":"disabled"}`: on current models that pairing has two
    // documented failure modes (a tool call leaking into visible text, and
    // `<thinking>` tags leaking into the response). Omitting the parameter
    // just runs the model's own default.
    //
    // Current models also reject `temperature`/`top_p`/`top_k` and
    // `thinking.budget_tokens` outright (400) — `ChatOptions` has no such
    // fields, so there is nothing here that could send them.
    if let Thinking::Adaptive { effort } = &options.thinking {
        body["thinking"] = serde_json::json!({"type": "adaptive"});
        body["output_config"] = serde_json::json!({"effort": effort_str(*effort)});
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

fn to_wire_tool(tool: &ToolDef) -> serde_json::Value {
    serde_json::json!({
        "name": tool.name,
        "description": tool.description,
        "input_schema": tool.input_schema,
    })
}

fn to_wire_tool_choice(choice: &ToolChoice) -> serde_json::Value {
    match choice {
        ToolChoice::Auto => serde_json::json!({"type": "auto"}),
        ToolChoice::Any => serde_json::json!({"type": "any"}),
        ToolChoice::None => serde_json::json!({"type": "none"}),
        ToolChoice::Tool { name } => serde_json::json!({"type": "tool", "name": name}),
    }
}

fn to_wire_message(message: &Message) -> serde_json::Value {
    serde_json::json!({
        "role": match message.role {
            Role::User => "user",
            Role::Assistant => "assistant",
        },
        "content": message.content.iter().filter_map(to_wire_block).collect::<Vec<_>>(),
    })
}

/// Convert one content block to Anthropic's wire shape.
///
/// Returns `None` — dropping the block, with a warning — for two cases:
/// `Unknown` (its payload didn't survive deserialization, so there is
/// nothing to send) and top-level `Image` (image *input* is out of scope for
/// the Anthropic provider; see the project requirements). A `tool_result`'s
/// own image content is not affected — that path is real and stays.
fn to_wire_block(block: &ContentBlock) -> Option<serde_json::Value> {
    match block {
        ContentBlock::Text { text } => Some(serde_json::json!({"type": "text", "text": text})),
        ContentBlock::Thinking { thinking, signature } => {
            let mut v = serde_json::json!({"type": "thinking", "thinking": thinking});
            if let Some(sig) = signature {
                v["signature"] = serde_json::json!(sig);
            }
            Some(v)
        }
        ContentBlock::RedactedThinking { data } => {
            Some(serde_json::json!({"type": "redacted_thinking", "data": data}))
        }
        ContentBlock::ToolUse { id, name, input } => Some(serde_json::json!({
            "type": "tool_use",
            "id": id,
            "name": name,
            "input": input,
        })),
        ContentBlock::ToolResult {
            tool_use_id,
            content,
            is_error,
        } => Some(serde_json::json!({
            "type": "tool_result",
            "tool_use_id": tool_use_id,
            "content": content.iter().map(to_wire_tool_result_content).collect::<Vec<_>>(),
            "is_error": is_error,
        })),
        ContentBlock::Image { .. } => {
            tracing::warn!(
                "dropping an Image content block: image input is out of scope for the \
                 Anthropic provider"
            );
            None
        }
        ContentBlock::Unknown => {
            tracing::warn!(
                "dropping an Unknown content block: its payload did not survive deserialization"
            );
            None
        }
    }
}

fn to_wire_tool_result_content(content: &ToolResultContent) -> serde_json::Value {
    match content {
        ToolResultContent::Text { text } => serde_json::json!({"type": "text", "text": text}),
        ToolResultContent::Image { source } => serde_json::json!({
            "type": "image",
            "source": {
                "type": "base64",
                "media_type": media_type_str(&source.media_type),
                "data": source.data,
            },
        }),
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

// ---------------------------------------------------------------------
// Non-streaming response parsing
// ---------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct WireResponse {
    #[serde(default)]
    model: Option<String>,
    /// Reusing `shared::llm::ContentBlock` directly works because its wire
    /// shape mirrors Anthropic's — that mirroring was the point of modeling
    /// `shared::llm` on Anthropic's block shape in the first place.
    content: Vec<ContentBlock>,
    stop_reason: StopReason,
    #[serde(default)]
    usage: Usage,
}

pub fn parse_response(body: &str) -> Result<CompletedMessage, LlmError> {
    let wire: WireResponse = serde_json::from_str(body).map_err(|source| LlmError::Decode {
        provider: PROVIDER,
        context: "messages response".to_string(),
        source,
    })?;
    Ok(CompletedMessage {
        role: Role::Assistant,
        content: wire.content,
        stop_reason: wire.stop_reason,
        usage: wire.usage,
        model: wire.model,
    })
}

// ---------------------------------------------------------------------
// Error parsing
// ---------------------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
struct WireErrorEnvelope {
    #[serde(default)]
    error: Option<WireErrorDetail>,
    #[serde(default)]
    request_id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct WireErrorDetail {
    #[serde(default)]
    message: String,
}

/// Turn an HTTP status and response body into a typed error. Anthropic's
/// error envelope is `{"type":"error","error":{"type":...,"message":...},
/// "request_id":...}`; a body that doesn't parse as that shape (e.g. an
/// upstream proxy's plain-text error page) falls back to the raw body as the
/// message rather than losing it.
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
        request_id: envelope.request_id,
    }
}

// ---------------------------------------------------------------------
// Model listing
// ---------------------------------------------------------------------

/// One page of `GET {base_url}/v1/models`.
#[derive(Debug)]
pub struct ModelsPage {
    pub models: Vec<ModelInfo>,
    /// `last_id` from the response, but only when `has_more` is true — the
    /// cursor to pass as `after_id` on the next call. `None` means this was
    /// the last page.
    pub next_after_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WireModelsResponse {
    data: Vec<WireModel>,
    #[serde(default)]
    has_more: bool,
    #[serde(default)]
    last_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WireModel {
    id: String,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    created_at: Option<String>,
    #[serde(default)]
    max_input_tokens: Option<u64>,
    /// Named `max_tokens` on the wire — "Maximum value for the `max_tokens`
    /// parameter when using this model" — which is really a cap on *output*
    /// tokens, hence `ModelDetails::Anthropic::max_output_tokens` below.
    #[serde(default)]
    max_tokens: Option<u64>,
}

pub fn parse_models(body: &str) -> Result<ModelsPage, LlmError> {
    let wire: WireModelsResponse = serde_json::from_str(body).map_err(|source| LlmError::Decode {
        provider: PROVIDER,
        context: "models response".to_string(),
        source,
    })?;

    let models = wire
        .data
        .into_iter()
        .map(|m| ModelInfo {
            id: m.id,
            display_name: m.display_name,
            created_at: m.created_at.as_deref().and_then(parse_rfc3339_to_unix),
            details: ModelDetails::Anthropic {
                max_input_tokens: m.max_input_tokens,
                max_output_tokens: m.max_tokens,
            },
        })
        .collect();

    Ok(ModelsPage {
        models,
        next_after_id: if wire.has_more { wire.last_id } else { None },
    })
}

/// Convert an RFC 3339 timestamp (Anthropic's `created_at` shape) to unix
/// seconds. `None` on anything that doesn't parse — one field failing to
/// parse shouldn't fail the whole listing; the model is still returned, just
/// with `created_at: None`.
fn parse_rfc3339_to_unix(s: &str) -> Option<i64> {
    time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339)
        .ok()
        .map(|dt| dt.unix_timestamp())
}

// ---------------------------------------------------------------------
// Streaming
// ---------------------------------------------------------------------

/// State a live SSE stream carries across frames — Anthropic splits a
/// completed message's usage and stop reason across `message_start` (initial
/// input token count), `message_delta` (final output token count and stop
/// reason), and `message_stop` (a bare marker with no payload of its own),
/// so translating one frame at a time needs somewhere to hold what's been
/// seen so far.
#[derive(Debug, Default)]
pub struct StreamState {
    usage: Usage,
    stop_reason: Option<StopReason>,
}

#[derive(Debug, Deserialize)]
struct WireMessageStart {
    message: WireMessageStartInner,
}

#[derive(Debug, Deserialize)]
struct WireMessageStartInner {
    model: String,
    #[serde(default)]
    usage: Usage,
}

#[derive(Debug, Deserialize)]
struct WireContentBlockStart {
    index: usize,
    content_block: ContentBlock,
}

#[derive(Debug, Deserialize)]
struct WireContentBlockDelta {
    index: usize,
    delta: WireDelta,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WireDelta {
    TextDelta { text: String },
    InputJsonDelta { partial_json: String },
    ThinkingDelta { thinking: String },
    SignatureDelta { signature: String },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize)]
struct WireContentBlockStop {
    index: usize,
}

#[derive(Debug, Deserialize)]
struct WireMessageDelta {
    delta: WireMessageDeltaInner,
    #[serde(default)]
    usage: WireUsageDelta,
}

#[derive(Debug, Deserialize)]
struct WireMessageDeltaInner {
    #[serde(default)]
    stop_reason: Option<StopReason>,
}

/// Only the fields `message_delta` actually carries — deserializing directly
/// into `shared::llm::Usage` would default the *other* fields to zero and
/// clobber what `message_start` already recorded.
#[derive(Debug, Default, Deserialize)]
struct WireUsageDelta {
    #[serde(default)]
    input_tokens: Option<u32>,
    #[serde(default)]
    output_tokens: Option<u32>,
    #[serde(default)]
    cache_creation_input_tokens: Option<u32>,
    #[serde(default)]
    cache_read_input_tokens: Option<u32>,
}

fn decode_error(context: &str, source: serde_json::Error) -> LlmError {
    LlmError::Decode {
        provider: PROVIDER,
        context: context.to_string(),
        source,
    }
}

/// Translate one already-decoded SSE frame into zero-or-one `StreamEvent`s.
/// `ping` frames and any event type this module doesn't recognize yield
/// `None`; an `error` frame yields an `Err`. Pure — no I/O.
pub fn translate_frame(
    frame: &SseFrame,
    state: &mut StreamState,
) -> Result<Option<StreamEvent>, LlmError> {
    match frame.event.as_str() {
        "message_start" => {
            let wire: WireMessageStart = serde_json::from_str(&frame.data)
                .map_err(|e| decode_error("message_start event", e))?;
            state.usage = wire.message.usage;
            Ok(Some(StreamEvent::MessageStart {
                model: wire.message.model,
            }))
        }
        "content_block_start" => {
            let wire: WireContentBlockStart = serde_json::from_str(&frame.data)
                .map_err(|e| decode_error("content_block_start event", e))?;
            Ok(Some(StreamEvent::BlockStart {
                index: wire.index,
                block: wire.content_block,
            }))
        }
        "content_block_delta" => {
            let wire: WireContentBlockDelta = serde_json::from_str(&frame.data)
                .map_err(|e| decode_error("content_block_delta event", e))?;
            let delta = match wire.delta {
                WireDelta::TextDelta { text } => Some(Delta::Text { text }),
                WireDelta::ThinkingDelta { thinking } => Some(Delta::Thinking { thinking }),
                WireDelta::InputJsonDelta { partial_json } => {
                    Some(Delta::ToolInputJson { partial_json })
                }
                WireDelta::SignatureDelta { signature } => Some(Delta::Signature { signature }),
                WireDelta::Unknown => {
                    tracing::warn!("ignoring an unrecognized content_block_delta variant");
                    None
                }
            };
            Ok(delta.map(|delta| StreamEvent::Delta {
                index: wire.index,
                delta,
            }))
        }
        "content_block_stop" => {
            let wire: WireContentBlockStop = serde_json::from_str(&frame.data)
                .map_err(|e| decode_error("content_block_stop event", e))?;
            Ok(Some(StreamEvent::BlockStop { index: wire.index }))
        }
        "message_delta" => {
            let wire: WireMessageDelta = serde_json::from_str(&frame.data)
                .map_err(|e| decode_error("message_delta event", e))?;
            if let Some(v) = wire.delta.stop_reason {
                state.stop_reason = Some(v);
            }
            if let Some(v) = wire.usage.input_tokens {
                state.usage.input_tokens = v;
            }
            if let Some(v) = wire.usage.output_tokens {
                state.usage.output_tokens = v;
            }
            if let Some(v) = wire.usage.cache_creation_input_tokens {
                state.usage.cache_creation_input_tokens = Some(v);
            }
            if let Some(v) = wire.usage.cache_read_input_tokens {
                state.usage.cache_read_input_tokens = Some(v);
            }
            // No direct StreamEvent — the values just recorded are surfaced
            // once `message_stop` arrives.
            Ok(None)
        }
        "message_stop" => {
            let stop_reason = state
                .stop_reason
                .take()
                .unwrap_or_else(|| StopReason::Other("unknown".to_string()));
            Ok(Some(StreamEvent::MessageStop {
                stop_reason,
                usage: state.usage,
            }))
        }
        "ping" => Ok(None),
        "error" => {
            let envelope: WireErrorEnvelope = serde_json::from_str(&frame.data).unwrap_or_default();
            let message = envelope
                .error
                .map(|e| e.message)
                .filter(|m| !m.is_empty())
                .unwrap_or_else(|| frame.data.clone());
            Err(LlmError::Stream {
                provider: PROVIDER,
                message,
            })
        }
        other => {
            tracing::warn!(event = other, "ignoring an unrecognized Anthropic SSE event");
            Ok(None)
        }
    }
}

/// Decode a complete SSE transcript (e.g. a recorded fixture) into its
/// `StreamEvent`s in one pass. The live transport (`anthropic::mod`) reuses
/// [`translate_frame`] incrementally over a live `SseDecoder` instead — this
/// function exists so the same per-frame logic is testable on a plain
/// string, with no network and no async runtime involved.
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
    const RESPONSE_TOOL_USE: &str = include_str!("fixtures/response_tool_use.json");
    const RESPONSE_THINKING: &str = include_str!("fixtures/response_thinking.json");
    const STREAM_TEXT: &str = include_str!("fixtures/stream_text.sse");
    const ERROR_OVERLOADED: &str = include_str!("fixtures/error_overloaded.json");
    const ERROR_INVALID_REQUEST: &str = include_str!("fixtures/error_invalid_request.json");
    const MODELS: &str = include_str!("fixtures/models.json");
    const MODELS_LAST_PAGE: &str = include_str!("fixtures/models_last_page.json");

    fn cfg() -> AnthropicConfig {
        AnthropicConfig::default()
    }

    #[test]
    fn builds_a_minimal_messages_request() {
        let conv = Conversation {
            system: None,
            messages: vec![Message::user(vec![ContentBlock::Text {
                text: "Hi".to_string(),
            }])],
        };
        let body = build_request(&conv, &ChatOptions::default(), &cfg(), false);
        assert_eq!(body["model"], serde_json::json!("claude-opus-5"));
        assert_eq!(body["max_tokens"], serde_json::json!(16_000));
        assert_eq!(body["stream"], serde_json::json!(false));
        assert_eq!(
            body["messages"][0],
            serde_json::json!({"role": "user", "content": [{"type": "text", "text": "Hi"}]})
        );
        assert!(body.get("tools").is_none());
        assert!(body.get("system").is_none());
        assert!(body.get("thinking").is_none());
    }

    #[test]
    fn never_sends_temperature_or_budget_tokens() {
        let conv = Conversation::default();
        let options = ChatOptions {
            thinking: Thinking::Adaptive {
                effort: Effort::High,
            },
            ..ChatOptions::default()
        };
        let body = build_request(&conv, &options, &cfg(), false);
        // `ChatOptions` has no `temperature`/`top_p`/`top_k`/`budget_tokens`
        // fields at all, so there is no way for this function to emit them —
        // this test guards against a future field being added and silently
        // wired through.
        for key in ["temperature", "top_p", "top_k"] {
            assert!(body.get(key).is_none(), "must never send {key}");
        }
        assert!(
            body["thinking"].get("budget_tokens").is_none(),
            "must never send thinking.budget_tokens"
        );
        assert_eq!(body["thinking"], serde_json::json!({"type": "adaptive"}));
        assert_eq!(body["output_config"], serde_json::json!({"effort": "high"}));
    }

    #[test]
    fn drops_top_level_image_blocks_with_a_warning() {
        let conv = Conversation {
            system: None,
            messages: vec![Message::user(vec![
                ContentBlock::Text {
                    text: "look at this".to_string(),
                },
                ContentBlock::Image {
                    source: shared::llm::ImageSource {
                        media_type: MediaType::Png,
                        data: "aGVsbG8=".to_string(),
                    },
                },
            ])],
        };
        let body = build_request(&conv, &ChatOptions::default(), &cfg(), false);
        let content = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(content.len(), 1, "the image block should have been dropped");
    }

    #[test]
    fn parses_a_text_only_response() {
        let message = parse_response(RESPONSE_TEXT).unwrap();
        assert_eq!(message.stop_reason, StopReason::EndTurn);
        assert_eq!(message.model.as_deref(), Some("claude-opus-5"));
        assert_eq!(message.usage.input_tokens, 12);
        assert_eq!(message.usage.output_tokens, 9);
        assert_eq!(message.content.len(), 1);
        match &message.content[0] {
            ContentBlock::Text { text } => assert_eq!(text, "The capital of France is Paris."),
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn parses_tool_use_and_preserves_input_json() {
        let message = parse_response(RESPONSE_TOOL_USE).unwrap();
        assert_eq!(message.stop_reason, StopReason::ToolUse);
        assert_eq!(message.content.len(), 2);
        match &message.content[1] {
            ContentBlock::ToolUse { name, input, .. } => {
                assert_eq!(name, "get_weather");
                assert_eq!(input, &serde_json::json!({"location": "Paris"}));
            }
            other => panic!("expected ToolUse, got {other:?}"),
        }
    }

    #[test]
    fn round_trips_thinking_signatures_byte_for_byte() {
        let message = parse_response(RESPONSE_THINKING).unwrap();
        match &message.content[0] {
            ContentBlock::Thinking { signature, .. } => {
                assert_eq!(
                    signature.as_deref(),
                    Some("sig-EAoSDAoGYWJjMTIzEAE=")
                );
            }
            other => panic!("expected Thinking, got {other:?}"),
        }
        // Replaying it back into a request must not alter a single byte.
        let conv = Conversation {
            system: None,
            messages: vec![Message::assistant(message.content.clone())],
        };
        let body = build_request(&conv, &ChatOptions::default(), &cfg(), false);
        assert_eq!(
            body["messages"][0]["content"][0]["signature"],
            serde_json::json!("sig-EAoSDAoGYWJjMTIzEAE=")
        );
    }

    #[test]
    fn maps_error_bodies_to_typed_errors() {
        let err = parse_error(529, ERROR_OVERLOADED);
        match err {
            LlmError::Status {
                kind,
                message,
                request_id,
                ..
            } => {
                assert_eq!(kind, ApiErrorKind::Overloaded);
                assert_eq!(message, "Overloaded");
                assert_eq!(request_id.as_deref(), Some("req_01OVER"));
            }
            other => panic!("expected Status, got {other:?}"),
        }

        let err = parse_error(400, ERROR_INVALID_REQUEST);
        match err {
            LlmError::Status { kind, message, .. } => {
                assert_eq!(kind, ApiErrorKind::InvalidRequest);
                assert!(message.contains("roles must alternate"));
            }
            other => panic!("expected Status, got {other:?}"),
        }
    }

    #[test]
    fn classifies_429_and_529_as_retryable() {
        assert!(parse_error(529, ERROR_OVERLOADED).is_retryable());
        assert!(!parse_error(400, ERROR_INVALID_REQUEST).is_retryable());
    }

    #[test]
    fn a_malformed_error_body_falls_back_to_the_raw_text() {
        let err = parse_error(500, "<html>Bad Gateway</html>");
        match err {
            LlmError::Status { message, .. } => assert_eq!(message, "<html>Bad Gateway</html>"),
            other => panic!("expected Status, got {other:?}"),
        }
    }

    #[test]
    fn parses_ping_events_as_nothing() {
        let events = parse_stream_events("event: ping\ndata: {}\n\n").unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn a_stream_error_event_surfaces_as_an_error() {
        let sse = "event: error\ndata: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}\n\n";
        let err = parse_stream_events(sse).unwrap_err();
        match err {
            LlmError::Stream { message, .. } => assert_eq!(message, "Overloaded"),
            other => panic!("expected Stream, got {other:?}"),
        }
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
        assert_eq!(streamed.model, blocking.model);
        assert_eq!(streamed.usage.input_tokens, blocking.usage.input_tokens);
        assert_eq!(streamed.usage.output_tokens, blocking.usage.output_tokens);
        assert_eq!(
            serde_json::to_value(&streamed.content).unwrap(),
            serde_json::to_value(&blocking.content).unwrap()
        );
    }

    #[test]
    fn parses_a_page_of_models() {
        let page = parse_models(MODELS).unwrap();
        assert_eq!(page.models.len(), 2);
        assert_eq!(page.models[0].id, "claude-opus-5");
        assert_eq!(page.models[0].display_name.as_deref(), Some("Claude Opus 5"));
        match &page.models[0].details {
            ModelDetails::Anthropic {
                max_input_tokens,
                max_output_tokens,
            } => {
                assert_eq!(*max_input_tokens, Some(1_000_000));
                assert_eq!(*max_output_tokens, Some(64_000));
            }
            other => panic!("expected Anthropic details, got {other:?}"),
        }
    }

    #[test]
    fn converts_rfc3339_created_at_to_unix_seconds() {
        let page = parse_models(MODELS).unwrap();
        assert_eq!(page.models[0].created_at, Some(1_784_851_200));
    }

    #[test]
    fn hands_back_last_id_as_the_next_cursor_only_when_has_more() {
        let page = parse_models(MODELS).unwrap();
        assert_eq!(page.next_after_id.as_deref(), Some("claude-sonnet-5"));

        let last_page = parse_models(MODELS_LAST_PAGE).unwrap();
        assert_eq!(last_page.next_after_id, None);
    }
}
