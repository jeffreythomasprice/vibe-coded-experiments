//! Ollama native `/api/chat` request/response shapes and pure conversions
//! to/from `shared::llm` types.
//!
//! Verified against a local Ollama v0.30.10 install and
//! `ollama/api/types.go`. Every function here is pure — no I/O — so it's
//! unit-tested directly on recorded fixtures under `fixtures/`. See
//! `ollama::mod` for the thin transport layer that calls these.
//!
//! **The one genuinely lossy conversion in this crate lives here.** Ollama's
//! `Message.content` is a flat `String`, not content blocks, so
//! `to_wire_messages` below has to decide how to flatten a `shared::llm`
//! conversation: text blocks concatenate, image blocks hoist into the
//! sibling `images` array, and each `tool_result` block becomes its own
//! separate `{"role":"tool", ...}` message. That conversion is deliberate and
//! tested, not accidental.

use std::collections::HashMap;

use serde::{Deserialize, Deserializer};

use shared::llm::{
    ChatOptions, CompletedMessage, ContentBlock, Conversation, Delta, Effort, Message,
    ModelDetails, ModelInfo, Role, StopReason, StreamEvent, Thinking, ToolDef, ToolResultContent,
    Usage,
};

use crate::llm::config::OllamaConfig;
use crate::llm::error::{ApiErrorKind, LlmError};

pub const PROVIDER: &str = "ollama";

// ---------------------------------------------------------------------
// Request building
// ---------------------------------------------------------------------

/// Build the JSON body for `POST {base_url}/api/chat`. Model and
/// `max_tokens` come from `options` — both required fields on
/// `ChatOptions`, since neither has a sensible config-level default (see
/// `crate::llm::config`'s module doc).
pub fn build_request(
    conversation: &Conversation,
    options: &ChatOptions,
    cfg: &OllamaConfig,
    stream: bool,
) -> serde_json::Value {
    let mut tool_names: HashMap<String, String> = HashMap::new();
    let mut messages = Vec::new();
    if let Some(system) = &conversation.system {
        messages.push(serde_json::json!({"role": "system", "content": system}));
    }
    messages.extend(to_wire_messages(&conversation.messages, &mut tool_names));

    let mut body = serde_json::json!({
        "model": options.model,
        "messages": messages,
        "stream": stream,
        "keep_alive": cfg.keep_alive,
    });

    if !options.tools.is_empty() {
        body["tools"] =
            serde_json::json!(options.tools.iter().map(to_wire_tool).collect::<Vec<_>>());
    }
    // Ollama's API has no `tool_choice` field at all — it accepts one
    // silently and does nothing with it. Rather than pretend to honor it,
    // `options.tool_choice` is simply not read here; see the provider notes
    // in `README.md`.
    // Unlike Anthropic (where explicitly disabling thinking on a
    // thinking-by-default model has documented failure modes and is best
    // left omitted), Ollama's `think` is a plain boolean/level switch with
    // no such pitfall — and leaving it unset does NOT mean "off": Ollama
    // falls back to a thinking-capable model's own default, which is
    // typically to think anyway (confirmed live against a local qwen3.5
    // install, which kept reasoning until it ran out of `max_tokens` with
    // `think` unset). So `Thinking::Off` sends an explicit `false` here,
    // rather than omitting the field.
    body["think"] = match &options.thinking {
        Thinking::Off => serde_json::json!(false),
        Thinking::Adaptive { effort } => serde_json::json!(think_value(*effort)),
    };

    let mut wire_options = serde_json::Map::new();
    wire_options.insert(
        "num_predict".to_string(),
        serde_json::json!(options.max_tokens),
    );
    if !options.stop_sequences.is_empty() {
        wire_options.insert(
            "stop".to_string(),
            serde_json::json!(options.stop_sequences),
        );
    }
    if !wire_options.is_empty() {
        body["options"] = serde_json::Value::Object(wire_options);
    }

    body
}

fn think_value(effort: Effort) -> &'static str {
    match effort {
        Effort::Low => "low",
        Effort::Medium => "medium",
        Effort::High => "high",
    }
}

fn to_wire_tool(tool: &ToolDef) -> serde_json::Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": tool.name,
            "description": tool.description,
            "parameters": tool.input_schema,
        },
    })
}

fn role_str(role: &Role) -> &'static str {
    match role {
        Role::User => "user",
        Role::Assistant => "assistant",
    }
}

/// Flatten every `shared::llm::Message` into zero or more Ollama wire
/// messages, preserving order: a run of text/thinking/image/tool_use content
/// collapses into one message, and each `tool_result` block interrupts that
/// run to become its own `{"role":"tool", ...}` message before the run
/// resumes. `tool_names` accumulates `tool_use.id -> name` across the whole
/// conversation so a later `tool_result` (which only carries the id) can
/// fill in Ollama's `tool_name` field.
fn to_wire_messages(
    messages: &[Message],
    tool_names: &mut HashMap<String, String>,
) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    for message in messages {
        let mut text = String::new();
        let mut images: Vec<String> = Vec::new();
        let mut thinking: Option<String> = None;
        let mut tool_calls: Vec<serde_json::Value> = Vec::new();

        for block in &message.content {
            match block {
                ContentBlock::Text { text: t } => {
                    if !text.is_empty() {
                        text.push('\n');
                    }
                    text.push_str(t);
                }
                ContentBlock::Thinking { thinking: t, .. } => {
                    thinking = Some(match thinking.take() {
                        Some(existing) => format!("{existing}\n{t}"),
                        None => t.clone(),
                    });
                }
                ContentBlock::RedactedThinking { .. } => {
                    tracing::warn!(
                        "dropping a RedactedThinking block: Ollama has no equivalent field"
                    );
                }
                ContentBlock::Image { source } => {
                    images.push(source.data.clone());
                }
                ContentBlock::ToolUse { id, name, input } => {
                    tool_names.insert(id.clone(), name.clone());
                    tool_calls.push(serde_json::json!({
                        "function": {"name": name, "arguments": input},
                    }));
                }
                ContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    is_error,
                } => {
                    flush_message(
                        &mut text,
                        &mut images,
                        &mut thinking,
                        &mut tool_calls,
                        &mut out,
                        message.role,
                    );
                    out.push(tool_result_message(tool_use_id, content, *is_error, tool_names));
                }
                ContentBlock::Unknown => {
                    tracing::warn!(
                        "dropping an Unknown content block: its payload did not survive deserialization"
                    );
                }
            }
        }
        flush_message(
            &mut text,
            &mut images,
            &mut thinking,
            &mut tool_calls,
            &mut out,
            message.role,
        );
    }
    out
}

/// Emit the accumulated text/images/thinking/tool_calls as one message, if
/// there's anything to emit, and reset the accumulators.
fn flush_message(
    text: &mut String,
    images: &mut Vec<String>,
    thinking: &mut Option<String>,
    tool_calls: &mut Vec<serde_json::Value>,
    out: &mut Vec<serde_json::Value>,
    role: Role,
) {
    if text.is_empty() && images.is_empty() && tool_calls.is_empty() && thinking.is_none() {
        return;
    }
    let mut msg = serde_json::json!({
        "role": role_str(&role),
        "content": std::mem::take(text),
    });
    if !images.is_empty() {
        msg["images"] = serde_json::json!(std::mem::take(images));
    }
    if let Some(t) = thinking.take() {
        msg["thinking"] = serde_json::json!(t);
    }
    if !tool_calls.is_empty() {
        msg["tool_calls"] = serde_json::json!(std::mem::take(tool_calls));
    }
    out.push(msg);
}

fn tool_result_message(
    tool_use_id: &str,
    content: &[ToolResultContent],
    is_error: bool,
    tool_names: &HashMap<String, String>,
) -> serde_json::Value {
    let mut text = String::new();
    let mut images = Vec::new();
    for c in content {
        match c {
            ToolResultContent::Text { text: t } => {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(t);
            }
            ToolResultContent::Image { source } => images.push(source.data.clone()),
        }
    }
    if is_error && !text.is_empty() {
        text = format!("Error: {text}");
    }
    let name = tool_names.get(tool_use_id).cloned().unwrap_or_default();
    if name.is_empty() {
        tracing::warn!(
            tool_use_id,
            "no matching tool_use found for this tool_result; sending an empty tool_name"
        );
    }
    let mut msg = serde_json::json!({
        "role": "tool",
        "content": text,
        "tool_name": name,
        "tool_call_id": tool_use_id,
    });
    if !images.is_empty() {
        msg["images"] = serde_json::json!(images);
    }
    msg
}

// ---------------------------------------------------------------------
// Response parsing — shared by the blocking response and each streamed line
// ---------------------------------------------------------------------

/// Ollama's non-streaming response and each NDJSON line of a streamed one
/// share this exact shape — the streamed form is just this object with
/// `done: false` and incremental `message.content`/`message.thinking`, until
/// a final `done: true` object carrying the metrics.
#[derive(Debug, Deserialize)]
struct WireChunk {
    #[serde(default)]
    model: Option<String>,
    message: WireMessage,
    #[serde(default)]
    done: bool,
    #[serde(default)]
    done_reason: Option<String>,
    #[serde(default)]
    prompt_eval_count: u32,
    #[serde(default)]
    eval_count: u32,
}

#[derive(Debug, Default, Deserialize)]
struct WireMessage {
    #[serde(default)]
    content: String,
    #[serde(default)]
    thinking: Option<String>,
    #[serde(default)]
    tool_calls: Vec<WireToolCall>,
}

#[derive(Debug, Deserialize)]
struct WireToolCall {
    function: WireToolCallFunction,
}

#[derive(Debug, Deserialize)]
struct WireToolCallFunction {
    name: String,
    #[serde(default)]
    arguments: serde_json::Value,
}

pub fn parse_response(body: &str) -> Result<CompletedMessage, LlmError> {
    let wire: WireChunk = serde_json::from_str(body).map_err(|source| LlmError::Decode {
        provider: PROVIDER,
        context: "chat response".to_string(),
        source,
    })?;

    let mut content = Vec::new();
    if let Some(thinking) = wire.message.thinking.filter(|t| !t.is_empty()) {
        content.push(ContentBlock::Thinking {
            thinking,
            signature: None,
        });
    }

    let mut saw_tool_use = false;
    if !wire.message.tool_calls.is_empty() {
        saw_tool_use = true;
        for (index, call) in wire.message.tool_calls.into_iter().enumerate() {
            content.push(ContentBlock::ToolUse {
                id: synth_tool_id(index),
                name: call.function.name,
                input: call.function.arguments,
            });
        }
    } else if let Some(recovered) = recover_leaked_tool_call(&wire.message.content) {
        tracing::warn!(
            "recovered a tool call that arrived as plain text instead of the structured \
             tool_calls field — a known failure mode of some small local models"
        );
        content.push(recovered);
        saw_tool_use = true;
    } else if !wire.message.content.is_empty() {
        content.push(ContentBlock::Text {
            text: wire.message.content,
        });
    }

    Ok(CompletedMessage {
        role: Role::Assistant,
        content,
        stop_reason: stop_reason(saw_tool_use, wire.done_reason.as_deref()),
        usage: Usage {
            input_tokens: wire.prompt_eval_count,
            output_tokens: wire.eval_count,
            cache_read_input_tokens: None,
            cache_creation_input_tokens: None,
        },
        model: wire.model,
    })
}

fn stop_reason(saw_tool_use: bool, done_reason: Option<&str>) -> StopReason {
    if saw_tool_use {
        return StopReason::ToolUse;
    }
    match done_reason {
        Some("stop") | None => StopReason::EndTurn,
        Some("length") => StopReason::MaxTokens,
        Some(other) => StopReason::Other(other.to_string()),
    }
}

fn synth_tool_id(index: usize) -> String {
    format!("ollama-tool-{index}")
}

/// Detect a tool call that a (usually small) local model emitted as plain
/// text instead of Ollama's structured `tool_calls` field — a known failure
/// mode (observed with `qwen2.5-coder:3b`). Recognizes a JSON object with a
/// `name` string and either a `parameters` or `arguments` object. Anything
/// else is left as ordinary text: a missed recovery, never a panic.
fn recover_leaked_tool_call(content: &str) -> Option<ContentBlock> {
    let value: serde_json::Value = serde_json::from_str(content.trim()).ok()?;
    let object = value.as_object()?;
    let name = object.get("name")?.as_str()?.to_string();
    let input = object
        .get("parameters")
        .or_else(|| object.get("arguments"))
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    Some(ContentBlock::ToolUse {
        id: synth_tool_id(0),
        name,
        input,
    })
}

// ---------------------------------------------------------------------
// Error parsing
// ---------------------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
struct WireErrorEnvelope {
    #[serde(default)]
    error: String,
}

/// Ollama's error body is a bare `{"error": "message"}`, not an envelope
/// with a type/code — so unlike the other two providers, there's no
/// `request_id` to recover here.
pub fn parse_error(status: u16, body: &str) -> LlmError {
    let kind = ApiErrorKind::from_status(status);
    let envelope: WireErrorEnvelope = serde_json::from_str(body).unwrap_or_default();
    let message = if envelope.error.is_empty() {
        body.to_string()
    } else {
        envelope.error
    };
    LlmError::Status {
        provider: PROVIDER,
        status,
        kind,
        message,
        request_id: None,
    }
}

// ---------------------------------------------------------------------
// Model listing
// ---------------------------------------------------------------------

/// `GET {base_url}/api/tags` — models already pulled and ready to run.
/// Verified against a live v0.30.10 install: it carries more than
/// `ollama/api/types.go` alone would suggest (`capabilities`, `details.
/// context_length`), and sometimes a `remote_model`/`remote_host` pair for a
/// cloud-backed model (e.g. `glm-5.2:cloud`) — neither is modeled here, since
/// nothing in `ModelDetails::OllamaLocal` needs them yet, but they must not
/// break parsing when present.
#[derive(Debug, Deserialize)]
struct WireTagsResponse {
    #[serde(default)]
    models: Vec<WireLocalModel>,
}

#[derive(Debug, Deserialize)]
struct WireLocalModel {
    name: String,
    #[serde(default)]
    modified_at: Option<String>,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    digest: String,
    #[serde(default)]
    details: WireLocalModelDetails,
    #[serde(default, deserialize_with = "null_as_default")]
    capabilities: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct WireLocalModelDetails {
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    family: Option<String>,
    #[serde(default, deserialize_with = "null_as_default")]
    families: Vec<String>,
    #[serde(default)]
    parameter_size: Option<String>,
    #[serde(default)]
    quantization_level: Option<String>,
    #[serde(default)]
    context_length: Option<u64>,
    /// The size of the vector an embedding model produces. Only present on
    /// embedding models — a chat model's `details` omits it entirely, so
    /// `#[serde(default)]` alone (no `null_as_default`) is enough here.
    #[serde(default)]
    embedding_length: Option<u32>,
}

/// `#[serde(default)]` alone only kicks in when a key is *absent* — Ollama
/// sometimes sends `"families": null` or `"capabilities": null` outright
/// (observed on models where family/capability detection comes up empty,
/// e.g. some embedding models), which a bare `Vec<String>` field rejects
/// with "invalid type: null, expected a sequence". This treats an explicit
/// `null` the same as a missing key.
fn null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::deserialize(deserializer)?.unwrap_or_default())
}

pub fn parse_tags(body: &str) -> Result<Vec<ModelInfo>, LlmError> {
    let wire: WireTagsResponse = serde_json::from_str(body).map_err(|source| LlmError::Decode {
        provider: PROVIDER,
        context: "tags response".to_string(),
        source,
    })?;

    Ok(wire
        .models
        .into_iter()
        .map(|m| ModelInfo {
            id: m.name,
            display_name: None,
            created_at: m.modified_at.as_deref().and_then(parse_rfc3339_to_unix),
            details: if m.capabilities.iter().any(|c| c == "embedding") {
                ModelDetails::OllamaLocalEmbedding {
                    size_bytes: m.size,
                    digest: m.digest,
                    family: m.details.family,
                    families: m.details.families,
                    parameter_size: m.details.parameter_size,
                    quantization_level: m.details.quantization_level,
                    format: m.details.format,
                    context_length: m.details.context_length,
                    embedding_length: m.details.embedding_length,
                    capabilities: m.capabilities,
                }
            } else {
                ModelDetails::OllamaLocal {
                    size_bytes: m.size,
                    digest: m.digest,
                    family: m.details.family,
                    families: m.details.families,
                    parameter_size: m.details.parameter_size,
                    quantization_level: m.details.quantization_level,
                    format: m.details.format,
                    context_length: m.details.context_length,
                    capabilities: m.capabilities,
                }
            },
        })
        .collect())
}

/// Convert an RFC 3339 timestamp (Ollama's `modified_at` shape, with a
/// numeric UTC offset rather than Anthropic's bare `Z`) to unix seconds.
/// `None` on anything that doesn't parse, so one model's unexpected
/// timestamp shape doesn't fail the whole listing.
fn parse_rfc3339_to_unix(s: &str) -> Option<i64> {
    time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339)
        .ok()
        .map(|dt| dt.unix_timestamp())
}

// ---------------------------------------------------------------------
// Streaming
// ---------------------------------------------------------------------

/// State an NDJSON stream carries across lines: which block index (if any)
/// is currently open for thinking text and for regular text, so repeated
/// content on later lines becomes a `Delta` against the same block rather
/// than a new one every time.
#[derive(Debug, Default)]
pub struct StreamState {
    next_index: usize,
    started: bool,
    thinking_index: Option<usize>,
    text_index: Option<usize>,
    saw_tool_use: bool,
}

/// Translate one decoded NDJSON line into the `StreamEvent`s it produces —
/// zero, one, or occasionally two (a block's first delta also opens it).
/// Pure — no I/O.
pub fn translate_line(line: &str, state: &mut StreamState) -> Result<Vec<StreamEvent>, LlmError> {
    let wire: WireChunk = serde_json::from_str(line).map_err(|source| LlmError::Decode {
        provider: PROVIDER,
        context: "streamed chat chunk".to_string(),
        source,
    })?;

    let mut events = Vec::new();

    if !state.started {
        state.started = true;
        events.push(StreamEvent::MessageStart {
            model: wire.model.clone().unwrap_or_default(),
        });
    }

    // Text and thinking each get their own block index, allocated lazily on
    // first use so a stream with no thinking at all never opens a block for
    // it.
    if let Some(thinking) = wire.message.thinking.as_deref().filter(|t| !t.is_empty()) {
        if state.thinking_index.is_none() {
            let index = allocate(&mut state.next_index);
            state.thinking_index = Some(index);
            events.push(StreamEvent::BlockStart {
                index,
                block: ContentBlock::Thinking {
                    thinking: String::new(),
                    signature: None,
                },
            });
        }
        events.push(StreamEvent::Delta {
            index: state.thinking_index.expect("just set above"),
            delta: Delta::Thinking {
                thinking: thinking.to_string(),
            },
        });
    }

    if !wire.message.content.is_empty() {
        if state.text_index.is_none() {
            let index = allocate(&mut state.next_index);
            state.text_index = Some(index);
            events.push(StreamEvent::BlockStart {
                index,
                block: ContentBlock::Text {
                    text: String::new(),
                },
            });
        }
        events.push(StreamEvent::Delta {
            index: state.text_index.expect("just set above"),
            delta: Delta::Text {
                text: wire.message.content.clone(),
            },
        });
    }

    for call in wire.message.tool_calls {
        state.saw_tool_use = true;
        let index = allocate(&mut state.next_index);
        events.push(StreamEvent::BlockStart {
            index,
            block: ContentBlock::ToolUse {
                id: synth_tool_id(index),
                name: call.function.name,
                input: call.function.arguments,
            },
        });
    }

    if wire.done {
        events.push(StreamEvent::MessageStop {
            stop_reason: stop_reason(state.saw_tool_use, wire.done_reason.as_deref()),
            usage: Usage {
                input_tokens: wire.prompt_eval_count,
                output_tokens: wire.eval_count,
                cache_read_input_tokens: None,
                cache_creation_input_tokens: None,
            },
        });
    }

    Ok(events)
}

fn allocate(next_index: &mut usize) -> usize {
    let index = *next_index;
    *next_index += 1;
    index
}

/// Decode a complete NDJSON transcript (e.g. a recorded fixture) into its
/// `StreamEvent`s in one pass — the counterpart of
/// `anthropic::wire::parse_stream_events`. The live transport
/// (`ollama::mod`) reuses [`translate_line`] incrementally over a live
/// `NdjsonDecoder` instead.
pub fn parse_stream_events(ndjson_text: &str) -> Result<Vec<StreamEvent>, LlmError> {
    let mut decoder = crate::llm::ndjson::NdjsonDecoder::new();
    let lines = decoder.push(ndjson_text.as_bytes());
    let mut state = StreamState::default();
    let mut events = Vec::new();
    for line in &lines {
        events.extend(translate_line(line, &mut state)?);
    }
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::accumulate::MessageAccumulator;
    use crate::llm::config::OllamaConfig;
    use shared::llm::{ImageSource, MediaType};

    const RESPONSE_TEXT: &str = include_str!("fixtures/response_text.json");
    const RESPONSE_TOOL_CALLS: &str = include_str!("fixtures/response_tool_calls.json");
    const RESPONSE_LEAKED_TOOL_CALL: &str =
        include_str!("fixtures/response_tool_call_leaked_as_text.json");
    const STREAM_TEXT: &str = include_str!("fixtures/stream_text.ndjson");
    const ERROR: &str = include_str!("fixtures/error.json");
    const TAGS: &str = include_str!("fixtures/tags.json");
    const TAGS_NULL_ARRAYS: &str = include_str!("fixtures/tags_null_arrays.json");

    fn cfg() -> OllamaConfig {
        OllamaConfig::default()
    }

    /// Only used to check that a request's model/max_tokens land on the
    /// wire unchanged — not a config default.
    const MODEL: &str = "llama3.1:8b";

    #[test]
    fn flattens_text_blocks_into_a_single_content_string() {
        let conv = Conversation {
            system: None,
            messages: vec![Message::user(vec![
                ContentBlock::Text {
                    text: "first line".to_string(),
                },
                ContentBlock::Text {
                    text: "second line".to_string(),
                },
            ])],
        };
        let body = build_request(&conv, &ChatOptions::new(MODEL, 1024), &cfg(), false);
        assert_eq!(
            body["messages"][0]["content"],
            serde_json::json!("first line\nsecond line")
        );
    }

    /// `max_tokens` is a required field on `ChatOptions` (see
    /// `crate::llm::config`'s module doc), so `options.num_predict` must
    /// always land on the wire — never omitted the way it used to be when
    /// the field was optional.
    #[test]
    fn num_predict_is_always_sent() {
        let body = build_request(
            &Conversation::default(),
            &ChatOptions::new(MODEL, 512),
            &cfg(),
            false,
        );
        assert_eq!(body["options"]["num_predict"], serde_json::json!(512));
    }

    #[test]
    fn hoists_image_blocks_into_the_images_array() {
        let conv = Conversation {
            system: None,
            messages: vec![Message::user(vec![
                ContentBlock::Text {
                    text: "what is this?".to_string(),
                },
                ContentBlock::Image {
                    source: ImageSource {
                        media_type: MediaType::Png,
                        data: "aGVsbG8=".to_string(),
                    },
                },
            ])],
        };
        let body = build_request(&conv, &ChatOptions::new(MODEL, 1024), &cfg(), false);
        assert_eq!(body["messages"][0]["content"], serde_json::json!("what is this?"));
        assert_eq!(
            body["messages"][0]["images"],
            serde_json::json!(["aGVsbG8="])
        );
    }

    #[test]
    fn renders_tool_results_as_tool_role_messages() {
        let conv = Conversation {
            system: None,
            messages: vec![
                Message::assistant(vec![ContentBlock::ToolUse {
                    id: "toolu_1".to_string(),
                    name: "get_weather".to_string(),
                    input: serde_json::json!({"location": "Paris"}),
                }]),
                Message::user(vec![ContentBlock::ToolResult {
                    tool_use_id: "toolu_1".to_string(),
                    content: vec![ToolResultContent::Text {
                        text: "72F and sunny".to_string(),
                    }],
                    is_error: false,
                }]),
            ],
        };
        let body = build_request(&conv, &ChatOptions::new(MODEL, 1024), &cfg(), false);
        let messages = body["messages"].as_array().unwrap();
        // assistant message with tool_calls, then a separate tool message —
        // never a "user" message wrapping the tool result.
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["role"], serde_json::json!("assistant"));
        assert_eq!(
            messages[0]["tool_calls"][0]["function"]["name"],
            serde_json::json!("get_weather")
        );
        assert_eq!(messages[1]["role"], serde_json::json!("tool"));
        assert_eq!(messages[1]["tool_name"], serde_json::json!("get_weather"));
        assert_eq!(messages[1]["tool_call_id"], serde_json::json!("toolu_1"));
        assert_eq!(messages[1]["content"], serde_json::json!("72F and sunny"));
    }

    #[test]
    fn ollama_tools_are_openai_shaped() {
        let tool = ToolDef {
            name: "get_weather".to_string(),
            description: "Get the weather".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        };
        let conv = Conversation::default();
        let options = ChatOptions {
            tools: vec![tool],
            ..ChatOptions::new(MODEL, 1024)
        };
        let body = build_request(&conv, &options, &cfg(), false);
        assert_eq!(body["tools"][0]["type"], serde_json::json!("function"));
        assert_eq!(
            body["tools"][0]["function"]["name"],
            serde_json::json!("get_weather")
        );
    }

    #[test]
    fn explicitly_disables_thinking_when_off_rather_than_omitting_it() {
        // Omitting `think` does not mean "off" for a thinking-capable
        // model — Ollama falls back to that model's own default, which is
        // typically to think anyway (confirmed live: qwen3.5 kept
        // reasoning with `think` unset until it ran out of `max_tokens`).
        let body = build_request(
            &Conversation::default(),
            &ChatOptions::new(MODEL, 1024),
            &cfg(),
            false,
        );
        assert_eq!(body["think"], serde_json::json!(false));
    }

    #[test]
    fn sends_the_requested_effort_level_when_thinking_is_adaptive() {
        let options = ChatOptions {
            thinking: Thinking::Adaptive {
                effort: Effort::High,
            },
            ..ChatOptions::new(MODEL, 1024)
        };
        let body = build_request(&Conversation::default(), &options, &cfg(), false);
        assert_eq!(body["think"], serde_json::json!("high"));
    }

    #[test]
    fn parses_tool_calls_with_object_arguments() {
        let message = parse_response(RESPONSE_TOOL_CALLS).unwrap();
        assert_eq!(message.stop_reason, StopReason::ToolUse);
        match &message.content[0] {
            ContentBlock::ToolUse { name, input, .. } => {
                assert_eq!(name, "get_weather");
                assert_eq!(input, &serde_json::json!({"location": "Paris"}));
            }
            other => panic!("expected ToolUse, got {other:?}"),
        }
    }

    #[test]
    fn recovers_when_a_small_model_leaks_a_tool_call_as_text() {
        let message = parse_response(RESPONSE_LEAKED_TOOL_CALL).unwrap();
        assert_eq!(message.stop_reason, StopReason::ToolUse);
        assert_eq!(message.content.len(), 1);
        match &message.content[0] {
            ContentBlock::ToolUse { name, input, .. } => {
                assert_eq!(name, "get_weather");
                assert_eq!(input, &serde_json::json!({"location": "Paris"}));
            }
            other => panic!("expected a recovered ToolUse, got {other:?}"),
        }
    }

    #[test]
    fn ordinary_text_that_is_not_json_is_never_treated_as_a_tool_call() {
        assert!(recover_leaked_tool_call("The weather in Paris is sunny.").is_none());
        assert!(recover_leaked_tool_call("{\"just\": \"an object\"}").is_none());
    }

    #[test]
    fn maps_metrics_into_usage() {
        let message = parse_response(RESPONSE_TEXT).unwrap();
        assert_eq!(message.usage.input_tokens, 14);
        assert_eq!(message.usage.output_tokens, 8);
    }

    #[test]
    fn surfaces_the_bare_error_field() {
        let err = parse_error(404, ERROR);
        match err {
            LlmError::Status { kind, message, request_id, .. } => {
                assert_eq!(kind, ApiErrorKind::NotFound);
                assert!(message.contains("not found"));
                assert_eq!(request_id, None);
            }
            other => panic!("expected Status, got {other:?}"),
        }
    }

    #[test]
    fn parses_ndjson_stream_into_deltas() {
        let events = parse_stream_events(STREAM_TEXT).unwrap();
        assert!(matches!(events[0], StreamEvent::MessageStart { .. }));
        let text_deltas: String = events
            .iter()
            .filter_map(|e| match e {
                StreamEvent::Delta {
                    delta: Delta::Text { text },
                    ..
                } => Some(text.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(text_deltas, "The capital of France is Paris.");
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
    fn parses_local_models_including_capabilities_and_context_length() {
        let models = parse_tags(TAGS).unwrap();
        assert_eq!(models.len(), 4);
        assert_eq!(models[0].id, "qwen3.8:latest");
        match &models[0].details {
            ModelDetails::OllamaLocal {
                size_bytes,
                parameter_size,
                context_length,
                capabilities,
                ..
            } => {
                assert_eq!(*size_bytes, 17_741_872_154);
                assert_eq!(parameter_size.as_deref(), Some("27.3B"));
                assert_eq!(*context_length, Some(262_144));
                assert_eq!(
                    capabilities,
                    &vec![
                        "completion".to_string(),
                        "tools".to_string(),
                        "thinking".to_string(),
                        "vision".to_string(),
                    ]
                );
            }
            other => panic!("expected OllamaLocal details, got {other:?}"),
        }
    }

    #[test]
    fn converts_modified_at_with_a_numeric_offset_to_unix_seconds() {
        let models = parse_tags(TAGS).unwrap();
        assert_eq!(models[0].created_at, Some(1_787_393_775));
    }

    #[test]
    fn tolerates_explicit_null_for_families_and_capabilities() {
        // Real Ollama installs sometimes send `"families": null` and/or
        // `"capabilities": null` (observed on models where detection comes
        // up empty) rather than omitting the key or sending `[]` — this used
        // to fail with "invalid type: null, expected a sequence".
        let models = parse_tags(TAGS_NULL_ARRAYS).unwrap();
        assert_eq!(models.len(), 1);
        match &models[0].details {
            ModelDetails::OllamaLocal {
                families,
                capabilities,
                ..
            } => {
                assert!(families.is_empty());
                assert!(capabilities.is_empty());
            }
            other => panic!("expected OllamaLocal details, got {other:?}"),
        }
    }

    #[test]
    fn tolerates_the_remote_model_and_remote_host_fields_on_a_cloud_backed_model() {
        // glm-5.2:cloud carries remote_model/remote_host on the wire — this
        // must parse without error even though neither field is modeled.
        let models = parse_tags(TAGS).unwrap();
        let cloud = models.iter().find(|m| m.id == "glm-5.2:cloud").unwrap();
        assert_eq!(cloud.created_at, Some(1_781_867_962));
    }

    #[test]
    fn a_model_whose_capabilities_include_embedding_classifies_as_ollama_local_embedding() {
        let models = parse_tags(TAGS).unwrap();
        let embedder = models
            .iter()
            .find(|m| m.id == "nomic-embed-text:latest")
            .unwrap();
        assert!(embedder.details.is_embedding());
        match &embedder.details {
            ModelDetails::OllamaLocalEmbedding {
                embedding_length,
                context_length,
                capabilities,
                ..
            } => {
                assert_eq!(*embedding_length, Some(768));
                assert_eq!(*context_length, Some(2048));
                assert_eq!(capabilities, &vec!["embedding".to_string()]);
            }
            other => panic!("expected OllamaLocalEmbedding details, got {other:?}"),
        }
        assert_eq!(embedder.details.embedding_dimensions(), Some(768));
        assert_eq!(embedder.details.max_input_tokens(), Some(2048));

        // A chat model, even one whose wire payload carries its own
        // (architectural, not embedding-task) embedding_length, must not be
        // classified as an embedding model.
        let chat = models.iter().find(|m| m.id == "llama3.1:8b").unwrap();
        assert!(!chat.details.is_embedding());
        assert!(matches!(chat.details, ModelDetails::OllamaLocal { .. }));
    }
}
