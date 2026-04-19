use async_trait::async_trait;
use futures_util::stream::BoxStream;
use rig::client::CompletionClient;
use rig::completion::CompletionModel as _;
use rig::providers::anthropic;
use secrecy::{ExposeSecret, SecretString};

use super::convert::{build_rig_request, completion_error_to_llm, rig_choice_to_response};
use super::error::LlmError;
use super::ollama::{adapt_stream, parse_structured, structured_output_system_prompt};
use super::provider::LlmProvider;
use super::types::{ChatRequest, ChatResponse, StreamChunk};

/// Anthropic requires a `max_tokens` value on every request; we default to
/// this when the caller doesn't supply one.
const DEFAULT_ANTHROPIC_MAX_TOKENS: u64 = 4096;

pub struct AnthropicProvider {
    client: anthropic::Client,
    model: String,
}

impl AnthropicProvider {
    pub fn new(api_key: &SecretString, model: &str) -> Result<Self, LlmError> {
        let client = anthropic::Client::new(api_key.expose_secret().to_string())
            .map_err(|e| LlmError::ConfigInvalid(format!("anthropic client: {e}")))?;
        Ok(Self {
            client,
            model: model.to_string(),
        })
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, LlmError> {
        let model = self.client.completion_model(&self.model);
        let (extra_system, schema) = structured_output_system_prompt(&req);
        let inputs = build_rig_request(&req, extra_system.as_deref())?;

        let rig_req = model
            .completion_request(inputs.prompt)
            .messages(inputs.chat_history)
            .tools(inputs.tools)
            .temperature_opt(inputs.temperature)
            .max_tokens(inputs.max_tokens.unwrap_or(DEFAULT_ANTHROPIC_MAX_TOKENS))
            .build();

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
        let model = self.client.completion_model(&self.model);
        let (extra_system, _schema) = structured_output_system_prompt(&req);
        let inputs = build_rig_request(&req, extra_system.as_deref())?;

        let rig_req = model
            .completion_request(inputs.prompt)
            .messages(inputs.chat_history)
            .tools(inputs.tools)
            .temperature_opt(inputs.temperature)
            .max_tokens(inputs.max_tokens.unwrap_or(DEFAULT_ANTHROPIC_MAX_TOKENS))
            .build();

        let stream = model
            .stream(rig_req)
            .await
            .map_err(completion_error_to_llm)?;
        Ok(adapt_stream(stream))
    }
}
