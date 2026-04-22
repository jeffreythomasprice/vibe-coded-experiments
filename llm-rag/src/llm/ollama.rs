use anyhow::anyhow;
use async_trait::async_trait;
use futures_util::StreamExt;
use futures_util::stream::BoxStream;
use rig::client::{CompletionClient, EmbeddingsClient, Nothing};
use rig::completion::CompletionModel as _;
use rig::embeddings::EmbeddingModel as _;
use rig::providers::ollama;
use secrecy::SecretString;

use super::convert::{
    RigRequestInputs, build_rig_request, completion_error_to_llm, rig_choice_to_response,
};
use super::error::LlmError;
use super::provider::{EmbeddingProvider, LlmProvider};
use super::types::{ChatRequest, ChatResponse, StreamChunk};

/// Result of an Ollama embedding-model probe: the vector length (always) and
/// the advertised context window in tokens (if `/api/show` reported one).
#[derive(Debug, Clone, Copy)]
pub struct OllamaEmbeddingProbe {
    pub dimensions: usize,
    pub max_input_tokens: Option<usize>,
}

/// Wraps a rig Ollama client. `chat_model` is populated for chat providers,
/// `embedding_model` is populated for embedding providers (a single struct
/// keeps the factory simple even though each instance has only one role).
pub struct OllamaProvider {
    client: ollama::Client,
    chat_model: Option<String>,
    embedding_model: Option<EmbeddingState>,
}

struct EmbeddingState {
    name: String,
    dimensions: usize,
    max_input_tokens: Option<usize>,
}

impl OllamaProvider {
    pub fn new(url: &str, chat_model: &str) -> Result<Self, LlmError> {
        let client = build_client(url)?;
        Ok(Self {
            client,
            chat_model: Some(chat_model.to_string()),
            embedding_model: None,
        })
    }

    pub fn new_embeddings(
        url: &str,
        model: &str,
        dimensions: usize,
        max_input_tokens: Option<usize>,
    ) -> Result<Self, LlmError> {
        let client = build_client(url)?;
        Ok(Self {
            client,
            chat_model: None,
            embedding_model: Some(EmbeddingState {
                name: model.to_string(),
                dimensions,
                max_input_tokens,
            }),
        })
    }
}

/// Issue a one-shot embedding call against Ollama and return the resulting
/// vector's length plus the model's advertised context length (if reported).
/// Used at server bootstrap to populate the `embedding_model_dimensions`
/// cache without forcing the operator to declare any model metadata in
/// `config.toml`.
///
/// rig's `embedding_model_with_ndims(name, n)` stores `n` as metadata only —
/// it is not sent in the HTTP request, so passing a placeholder `1` is
/// harmless. The vector length we read back is what the model actually
/// produces.
///
/// The context length comes from Ollama's `/api/show` endpoint, which returns
/// a `model_info` map whose context key is architecture-specific (e.g.
/// `bert.context_length`, `nomic-bert.context_length`). If the call fails or
/// no matching key is present, `max_input_tokens` is `None` and the caller
/// falls back to a conservative chunk-size default.
pub async fn probe_ollama_embedding_info(
    url: &str,
    model: &str,
) -> Result<OllamaEmbeddingProbe, LlmError> {
    let client = build_client(url)?;
    let probe_model = client.embedding_model_with_ndims(model, 1);
    let embedding = probe_model
        .embed_text(" ")
        .await
        .map_err(|e| LlmError::Provider(anyhow!(e)))?;
    let dimensions = embedding.vec.len();

    // /api/show is best-effort: if the endpoint is unreachable or the payload
    // doesn't expose a context length, we log and continue with None.
    let max_input_tokens = match fetch_ollama_context_length(url, model).await {
        Ok(v) => v,
        Err(err) => {
            tracing::warn!(
                model = %model,
                error = %err,
                "failed to fetch /api/show context length; falling back to default chunk size",
            );
            None
        }
    };

    Ok(OllamaEmbeddingProbe {
        dimensions,
        max_input_tokens,
    })
}

async fn fetch_ollama_context_length(url: &str, model: &str) -> Result<Option<usize>, LlmError> {
    let endpoint = format!("{}/api/show", url.trim_end_matches('/'));
    let resp = reqwest::Client::new()
        .post(endpoint)
        .json(&serde_json::json!({ "name": model }))
        .send()
        .await
        .map_err(|e| LlmError::Provider(anyhow!(e)))?;
    if !resp.status().is_success() {
        return Err(LlmError::Provider(anyhow!(
            "ollama /api/show returned {}",
            resp.status()
        )));
    }
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| LlmError::Provider(anyhow!(e)))?;
    Ok(extract_context_length(&body))
}

/// Pick the first `*.context_length` integer field out of an Ollama
/// `/api/show` response. Returns `None` if no such field is present.
fn extract_context_length(body: &serde_json::Value) -> Option<usize> {
    let info = body.get("model_info")?.as_object()?;
    for (key, value) in info {
        if key.ends_with(".context_length")
            && let Some(n) = value.as_u64()
        {
            return Some(n as usize);
        }
    }
    None
}

fn build_client(url: &str) -> Result<ollama::Client, LlmError> {
    ollama::Client::builder()
        .api_key(Nothing)
        .base_url(url)
        .build()
        .map_err(|e| LlmError::ConfigInvalid(format!("ollama client: {e}")))
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, LlmError> {
        let model_id = self
            .chat_model
            .as_deref()
            .ok_or(LlmError::Unsupported("ollama chat not configured"))?;
        let model = self.client.completion_model(model_id);

        let (extra_system, schema) = structured_output_system_prompt(&req);
        let inputs = build_rig_request(&req, extra_system.as_deref())?;
        let rig_req = assemble_completion_request(&model, inputs);

        let resp = model
            .completion(rig_req)
            .await
            .map_err(completion_error_to_llm)?;

        let mut out = rig_choice_to_response(&resp.choice)?;
        if schema.is_some() {
            out.structured = Some(parse_structured(&out.text)?);
        }
        Ok(out)
    }

    async fn chat_stream(
        &self,
        req: ChatRequest,
    ) -> Result<BoxStream<'static, Result<StreamChunk, LlmError>>, LlmError> {
        let model_id = self
            .chat_model
            .as_deref()
            .ok_or(LlmError::Unsupported("ollama chat not configured"))?;
        let model = self.client.completion_model(model_id);

        let (extra_system, _schema) = structured_output_system_prompt(&req);
        let inputs = build_rig_request(&req, extra_system.as_deref())?;
        let rig_req = assemble_completion_request(&model, inputs);

        let stream = model
            .stream(rig_req)
            .await
            .map_err(completion_error_to_llm)?;
        Ok(adapt_stream(stream))
    }
}

#[async_trait]
impl EmbeddingProvider for OllamaProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, LlmError> {
        let state = self
            .embedding_model
            .as_ref()
            .ok_or(LlmError::Unsupported("ollama embeddings not configured"))?;
        let model = self
            .client
            .embedding_model_with_ndims(&state.name, state.dimensions);
        let embedding = model
            .embed_text(text)
            .await
            .map_err(|e| LlmError::Provider(anyhow!(e)))?;
        Ok(embedding.vec.into_iter().map(|v| v as f32).collect())
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, LlmError> {
        let state = self
            .embedding_model
            .as_ref()
            .ok_or(LlmError::Unsupported("ollama embeddings not configured"))?;
        let model = self
            .client
            .embedding_model_with_ndims(&state.name, state.dimensions);
        let embeddings = model
            .embed_texts(texts.to_vec())
            .await
            .map_err(|e| LlmError::Provider(anyhow!(e)))?;
        Ok(embeddings
            .into_iter()
            .map(|e| e.vec.into_iter().map(|v| v as f32).collect())
            .collect())
    }

    fn max_input_tokens(&self) -> Option<usize> {
        self.embedding_model
            .as_ref()
            .and_then(|s| s.max_input_tokens)
    }
}

fn assemble_completion_request<M: rig::completion::CompletionModel>(
    model: &M,
    inputs: RigRequestInputs,
) -> rig::completion::CompletionRequest {
    let mut builder = model
        .completion_request(inputs.prompt)
        .messages(inputs.chat_history)
        .tools(inputs.tools)
        .temperature_opt(inputs.temperature)
        .max_tokens_opt(inputs.max_tokens);
    let _ = &mut builder;
    builder.build()
}

pub(super) fn structured_output_system_prompt(
    req: &ChatRequest,
) -> (Option<String>, Option<&serde_json::Value>) {
    match &req.json_schema {
        Some(schema) => {
            let msg = format!(
                "You must respond with a single JSON value conforming to this JSON Schema. \
                 Do not include any explanation, markdown, or code fences — only the JSON.\n\n\
                 Schema:\n{}",
                serde_json::to_string(schema).unwrap_or_else(|_| "{}".into())
            );
            (Some(msg), Some(schema))
        }
        None => (None, None),
    }
}

pub(super) fn parse_structured(text: &str) -> Result<serde_json::Value, LlmError> {
    let trimmed = strip_code_fences(text.trim());
    serde_json::from_str::<serde_json::Value>(trimmed).map_err(|source| LlmError::StructuredParse {
        source,
        raw: text.to_string(),
    })
}

fn strip_code_fences(s: &str) -> &str {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("```json") {
        rest.trim_end_matches("```").trim()
    } else if let Some(rest) = s.strip_prefix("```") {
        rest.trim_end_matches("```").trim()
    } else {
        s
    }
}

pub(super) fn adapt_stream<R>(
    stream: rig::streaming::StreamingCompletionResponse<R>,
) -> BoxStream<'static, Result<StreamChunk, LlmError>>
where
    R: Clone + Unpin + Send + 'static + rig::completion::GetTokenUsage,
{
    use rig::streaming::{StreamedAssistantContent, ToolCallDeltaContent};

    let mapped = stream.filter_map(|item| async move {
        match item {
            Err(e) => Some(Err(completion_error_to_llm(e))),
            Ok(chunk) => match chunk {
                StreamedAssistantContent::Text(t) => Some(Ok(StreamChunk::Text(t.text))),
                StreamedAssistantContent::ToolCall {
                    tool_call,
                    internal_call_id: _,
                } => Some(Ok(StreamChunk::ToolCallDelta {
                    id: tool_call.id,
                    name: Some(tool_call.function.name),
                    arguments_delta: tool_call.function.arguments.to_string(),
                })),
                StreamedAssistantContent::ToolCallDelta {
                    id,
                    internal_call_id: _,
                    content,
                } => match content {
                    ToolCallDeltaContent::Name(name) => Some(Ok(StreamChunk::ToolCallDelta {
                        id,
                        name: Some(name),
                        arguments_delta: String::new(),
                    })),
                    ToolCallDeltaContent::Delta(delta) => Some(Ok(StreamChunk::ToolCallDelta {
                        id,
                        name: None,
                        arguments_delta: delta,
                    })),
                },
                StreamedAssistantContent::Reasoning(_)
                | StreamedAssistantContent::ReasoningDelta { .. }
                | StreamedAssistantContent::Final(_) => None,
            },
        }
    });

    Box::pin(mapped.chain(futures_util::stream::once(async { Ok(StreamChunk::Done) })))
}

// The `SecretString` parameter is reserved for future auth-required Ollama
// deployments. Unused warning suppressed by taking `&SecretString` out of the
// signature entirely; kept here as a documentation anchor.
#[allow(dead_code)]
fn _unused_secret_marker(_k: &SecretString) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::types::Message;
    use serde_json::json;

    #[test]
    fn structured_output_parses_valid_json() {
        let parsed = parse_structured(r#"{"hello": "world"}"#).unwrap();
        assert_eq!(parsed, json!({"hello": "world"}));
    }

    #[test]
    fn structured_output_strips_json_code_fence() {
        let parsed =
            parse_structured("```json\n{\"k\": 1}\n```").expect("fenced json should parse");
        assert_eq!(parsed, json!({"k": 1}));
    }

    #[test]
    fn structured_output_surfaces_raw_on_parse_failure() {
        let err = parse_structured("not json at all").unwrap_err();
        match err {
            LlmError::StructuredParse { raw, .. } => {
                assert!(raw.contains("not json at all"));
            }
            other => panic!("expected StructuredParse, got {other:?}"),
        }
    }

    #[test]
    fn structured_output_system_prompt_set_when_schema_present() {
        let req = ChatRequest {
            messages: vec![Message::user("x")],
            json_schema: Some(json!({"type": "object"})),
            ..Default::default()
        };
        let (prompt, schema) = structured_output_system_prompt(&req);
        assert!(prompt.unwrap().contains("JSON Schema"));
        assert!(schema.is_some());
    }

    #[test]
    fn structured_output_system_prompt_absent_when_no_schema() {
        let req = ChatRequest {
            messages: vec![Message::user("x")],
            ..Default::default()
        };
        let (prompt, schema) = structured_output_system_prompt(&req);
        assert!(prompt.is_none());
        assert!(schema.is_none());
    }

    #[test]
    fn extract_context_length_reads_architecture_specific_key() {
        let body = json!({
            "model_info": {
                "general.architecture": "bert",
                "bert.context_length": 2048,
                "bert.embedding_length": 768,
            }
        });
        assert_eq!(extract_context_length(&body), Some(2048));
    }

    #[test]
    fn extract_context_length_matches_any_suffix() {
        let body = json!({
            "model_info": {
                "nomic-bert.context_length": 8192,
            }
        });
        assert_eq!(extract_context_length(&body), Some(8192));
    }

    #[test]
    fn extract_context_length_returns_none_when_missing() {
        let body = json!({ "model_info": { "bert.embedding_length": 768 } });
        assert_eq!(extract_context_length(&body), None);
    }

    #[test]
    fn extract_context_length_returns_none_without_model_info() {
        let body = json!({ "modelfile": "FROM ...", "template": "..." });
        assert_eq!(extract_context_length(&body), None);
    }
}
