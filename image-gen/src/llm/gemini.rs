use async_trait::async_trait;
use serde_json::{Value, json};

use crate::gemini::Client;
use crate::gemini::client::{RawCandidate, RawLogprobsResult, RawResponse};
use crate::llm::LlmError;
use crate::llm::message::{Content, Message, Role};
use crate::llm::provider::{
    ChatOptions, ChatRequest, ChatResponse, Provider, StopReason, TokenChoice, TokenLogprob, Usage,
};
use crate::llm::tool::{ToolCall, ToolDef};

const PROVIDER_NAME: &str = "gemini";

pub struct GeminiProvider {
    client: Client,
}

impl GeminiProvider {
    pub fn new(client: Client) -> Self {
        Self { client }
    }
}

#[async_trait]
impl Provider for GeminiProvider {
    fn name(&self) -> &'static str {
        PROVIDER_NAME
    }

    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, LlmError> {
        let body = chat_body(request);
        let raw = self.client.generate_content(&request.model, &body).await?;
        Ok(chat_response(raw)?)
    }
}

fn chat_body(request: &ChatRequest) -> Value {
    let system: Vec<String> = request
        .messages
        .iter()
        .filter(|message| message.role == Role::System)
        .map(Message::text)
        .collect();
    let contents: Vec<Value> = request
        .messages
        .iter()
        .filter(|message| message.role != Role::System)
        .map(message_to_wire)
        .collect();

    let mut body = json!({ "contents": contents });

    if !system.is_empty() {
        body["systemInstruction"] = json!({ "parts": [{ "text": system.join("\n\n") }] });
    }

    if !request.tools.is_empty() {
        body["tools"] = json!([{
            "functionDeclarations": request.tools.iter().map(tool_to_wire).collect::<Vec<_>>()
        }]);
    }

    let generation_config = generation_config(&request.options);
    if generation_config.as_object().is_some_and(|map| !map.is_empty()) {
        body["generationConfig"] = generation_config;
    }

    body
}

fn message_to_wire(message: &Message) -> Value {
    let mut parts: Vec<Value> = Vec::new();

    match message.role {
        Role::Tool => {
            parts.push(json!({
                "functionResponse": {
                    "name": message.tool_name.clone().unwrap_or_default(),
                    "response": { "content": message.text() },
                }
            }));
            parts.extend(message.images().map(image_part));
        }
        _ => {
            for part in &message.content {
                match part {
                    Content::Text(text) if text.is_empty() => {}
                    Content::Text(text) => parts.push(json!({ "text": text })),
                    Content::Image(image) => parts.push(image_part(image)),
                }
            }
            for call in &message.tool_calls {
                parts.push(json!({ "functionCall": { "name": call.name, "args": call.arguments } }));
            }
        }
    }

    json!({ "role": role_to_wire(message.role), "parts": parts })
}

fn image_part(image: &crate::llm::Image) -> Value {
    json!({ "inlineData": { "mimeType": image.media_type.as_mime(), "data": image.to_base64() } })
}

fn role_to_wire(role: Role) -> &'static str {
    match role {
        Role::Assistant => "model",
        Role::System | Role::User | Role::Tool => "user",
    }
}

fn tool_to_wire(def: &ToolDef) -> Value {
    json!({
        "name": def.name,
        "description": def.description,
        "parametersJsonSchema": def.schema,
    })
}

fn generation_config(options: &ChatOptions) -> Value {
    let mut map = serde_json::Map::new();
    if let Some(temperature) = options.temperature {
        map.insert("temperature".to_owned(), json!(temperature));
    }
    if let Some(top_p) = options.top_p {
        map.insert("topP".to_owned(), json!(top_p));
    }
    if let Some(seed) = options.seed {
        map.insert("seed".to_owned(), json!(seed));
    }
    if let Some(max_tokens) = options.max_tokens {
        map.insert("maxOutputTokens".to_owned(), json!(max_tokens));
    }
    if !options.stop.is_empty() {
        map.insert("stopSequences".to_owned(), json!(options.stop));
    }
    if let Some(n) = options.logprobs {
        map.insert("responseLogprobs".to_owned(), json!(true));
        map.insert("logprobs".to_owned(), json!(n));
    }
    // A budget of 0 turns thinking off, -1 lets the model decide.
    if let Some(think) = options.think {
        let budget = if think { -1 } else { 0 };
        map.insert("thinkingConfig".to_owned(), json!({ "thinkingBudget": budget }));
    }
    if let Some(format) = &options.format {
        map.insert("responseMimeType".to_owned(), json!("application/json"));
        map.insert("responseJsonSchema".to_owned(), format.clone());
    }
    Value::Object(map)
}

/// Gemini's `functionCall` parts carry no id, so ids are synthesized to give the
/// neutral `ToolCall` the same shape across providers.
fn chat_response(raw: RawResponse) -> Result<ChatResponse, crate::gemini::GeminiError> {
    let usage = raw.usage_metadata.as_ref().map_or_else(Usage::default, |usage| Usage {
        prompt_tokens: usage.prompt_token_count,
        completion_tokens: usage.candidates_token_count,
    });
    let candidate = raw.first_candidate()?;

    let tool_calls: Vec<ToolCall> = candidate
        .parts()
        .iter()
        .filter(|part| !part.thought)
        .filter_map(|part| part.function_call.as_ref())
        .enumerate()
        .map(|(index, call)| ToolCall {
            id: format!("call_{index}"),
            name: call.name.clone(),
            arguments: call.args.clone(),
        })
        .collect();

    let stop_reason = if !tool_calls.is_empty() {
        StopReason::ToolCalls
    } else {
        match candidate.finish_reason.as_deref() {
            Some("STOP") | None => StopReason::Stop,
            Some("MAX_TOKENS") => StopReason::Length,
            Some(other) => StopReason::Other(other.to_owned()),
        }
    };

    let logprobs = logprobs(&candidate);
    let message = Message {
        role: Role::Assistant,
        content: vec![Content::text(candidate.text())],
        tool_calls,
        tool_call_id: None,
        tool_name: None,
    };

    Ok(ChatResponse {
        message,
        stop_reason,
        usage,
        logprobs,
    })
}

fn logprobs(candidate: &RawCandidate) -> Vec<TokenChoice> {
    let Some(RawLogprobsResult {
        chosen_candidates,
        top_candidates,
    }) = &candidate.logprobs_result
    else {
        return Vec::new();
    };

    chosen_candidates
        .iter()
        .enumerate()
        .map(|(index, chosen)| TokenChoice {
            token: chosen.token.clone(),
            logprob: chosen.log_probability,
            top: top_candidates
                .get(index)
                .map(|top| {
                    top.candidates
                        .iter()
                        .map(|token| TokenLogprob {
                            token: token.token.clone(),
                            logprob: token.log_probability,
                        })
                        .collect()
                })
                .unwrap_or_default(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gemini::GeminiConfig;
    use crate::llm::message::{Image, MediaType};
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn request(messages: Vec<Message>) -> ChatRequest {
        ChatRequest {
            model: "gemini-2.5-flash".to_owned(),
            messages,
            tools: Vec::new(),
            options: ChatOptions::default(),
        }
    }

    fn provider_for(server: &MockServer) -> GeminiProvider {
        let client = Client::new(&GeminiConfig {
            api_key: Some("k".to_owned()),
            base_url: server.uri(),
            timeout_secs: 5,
        })
        .unwrap();
        GeminiProvider::new(client)
    }

    #[test]
    fn system_messages_become_system_instruction() {
        let body = chat_body(&request(vec![Message::system("be brief"), Message::user("hi")]));
        assert_eq!(body["systemInstruction"]["parts"][0]["text"], "be brief");
        let contents = body["contents"].as_array().unwrap();
        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0]["role"], "user");
        assert_eq!(contents[0]["parts"][0]["text"], "hi");
    }

    #[test]
    fn no_optional_sections_when_unused() {
        let body = chat_body(&request(vec![Message::user("hi")]));
        assert!(body.get("systemInstruction").is_none());
        assert!(body.get("tools").is_none());
        assert!(body.get("generationConfig").is_none());
    }

    #[test]
    fn images_become_inline_data() {
        let image = Image {
            media_type: MediaType::Jpeg,
            data: vec![1, 2, 3],
        };
        let body = chat_body(&request(vec![Message::user_with_images("look", vec![image])]));
        let inline = &body["contents"][0]["parts"][1]["inlineData"];
        assert_eq!(inline["mimeType"], "image/jpeg");
        assert_eq!(inline["data"], "AQID");
    }

    #[test]
    fn assistant_tool_calls_and_tool_results_round_trip_to_wire() {
        let call = ToolCall {
            id: "call_0".to_owned(),
            name: "lookup".to_owned(),
            arguments: json!({"q": "x"}),
        };
        let messages = vec![
            Message::user("go"),
            Message::assistant_with_tool_calls("", vec![call.clone()]),
            Message::tool_result(&call, vec![Content::text("42")]),
        ];
        let body = chat_body(&request(messages));
        let contents = body["contents"].as_array().unwrap();

        assert_eq!(contents[1]["role"], "model");
        assert_eq!(contents[1]["parts"].as_array().unwrap().len(), 1);
        assert_eq!(contents[1]["parts"][0]["functionCall"]["name"], "lookup");
        assert_eq!(contents[1]["parts"][0]["functionCall"]["args"]["q"], "x");
        assert_eq!(contents[2]["role"], "user");
        assert_eq!(contents[2]["parts"][0]["functionResponse"]["name"], "lookup");
        assert_eq!(contents[2]["parts"][0]["functionResponse"]["response"]["content"], "42");
    }

    #[test]
    fn tools_become_function_declarations() {
        let mut req = request(vec![Message::user("hi")]);
        req.tools.push(ToolDef {
            name: "lookup".to_owned(),
            description: "look it up".to_owned(),
            schema: json!({"type": "object"}),
        });
        let body = chat_body(&req);
        let decl = &body["tools"][0]["functionDeclarations"][0];
        assert_eq!(decl["name"], "lookup");
        assert_eq!(decl["description"], "look it up");
        assert_eq!(decl["parametersJsonSchema"]["type"], "object");
    }

    #[test]
    fn options_map_to_generation_config() {
        let mut req = request(vec![Message::user("hi")]);
        req.options = ChatOptions {
            temperature: Some(0.2),
            top_p: Some(0.9),
            seed: Some(5),
            max_tokens: Some(1),
            stop: vec!["END".to_owned()],
            logprobs: Some(20),
            think: Some(false),
            format: Some(json!({"type": "object"})),
        };
        let config = &chat_body(&req)["generationConfig"];

        assert_eq!(config["topP"].as_f64().unwrap() as f32, 0.9);
        assert_eq!(config["seed"], 5);
        assert_eq!(config["maxOutputTokens"], 1);
        assert_eq!(config["stopSequences"][0], "END");
        assert_eq!(config["responseLogprobs"], true);
        assert_eq!(config["logprobs"], 20);
        assert_eq!(config["thinkingConfig"]["thinkingBudget"], 0);
        assert_eq!(config["responseMimeType"], "application/json");
        assert_eq!(config["responseJsonSchema"]["type"], "object");
    }

    #[test]
    fn think_true_lets_the_model_decide() {
        let mut req = request(vec![Message::user("hi")]);
        req.options.think = Some(true);
        assert_eq!(chat_body(&req)["generationConfig"]["thinkingConfig"]["thinkingBudget"], -1);
    }

    #[tokio::test]
    async fn plain_reply_is_parsed() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/models/gemini-2.5-flash:generateContent"))
            .and(header("x-goog-api-key", "k"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "candidates": [{"content": {"parts": [
                    {"text": "reasoning", "thought": true},
                    {"text": "pong"}
                ]}, "finishReason": "STOP"}],
                "usageMetadata": {"promptTokenCount": 4, "candidatesTokenCount": 2}
            })))
            .mount(&server)
            .await;

        let response = provider_for(&server)
            .chat(&request(vec![Message::user("ping")]))
            .await
            .unwrap();
        assert_eq!(response.message.text(), "pong");
        assert_eq!(response.stop_reason, StopReason::Stop);
        assert_eq!(response.usage, Usage { prompt_tokens: 4, completion_tokens: 2 });
        assert!(response.logprobs.is_empty());
    }

    #[tokio::test]
    async fn function_calls_become_tool_calls() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "candidates": [{"content": {"parts": [
                    {"functionCall": {"name": "lookup", "args": {"q": "x"}}},
                    {"functionCall": {"name": "other", "args": {}}}
                ]}, "finishReason": "STOP"}]
            })))
            .mount(&server)
            .await;

        let response = provider_for(&server)
            .chat(&request(vec![Message::user("go")]))
            .await
            .unwrap();
        assert_eq!(response.stop_reason, StopReason::ToolCalls);
        assert_eq!(response.message.tool_calls.len(), 2);
        assert_eq!(response.message.tool_calls[0].id, "call_0");
        assert_eq!(response.message.tool_calls[1].id, "call_1");
        assert_eq!(response.message.tool_calls[0].arguments["q"], "x");
    }

    #[tokio::test]
    async fn max_tokens_and_logprobs_are_parsed() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "candidates": [{
                    "content": {"parts": [{"text": "Yes"}]},
                    "finishReason": "MAX_TOKENS",
                    "logprobsResult": {
                        "chosenCandidates": [{"token": "Yes", "logProbability": -0.1}],
                        "topCandidates": [{"candidates": [
                            {"token": "Yes", "logProbability": -0.1},
                            {"token": "No", "logProbability": -3.0}
                        ]}]
                    }
                }]
            })))
            .mount(&server)
            .await;

        let response = provider_for(&server)
            .chat(&request(vec![Message::user("ok?")]))
            .await
            .unwrap();
        assert_eq!(response.stop_reason, StopReason::Length);
        assert_eq!(response.logprobs.len(), 1);
        assert_eq!(response.logprobs[0].token, "Yes");
        assert_eq!(response.logprobs[0].top.len(), 2);
        assert_eq!(response.logprobs[0].top[1].token, "No");
        assert_eq!(response.logprobs[0].top[1].logprob, -3.0);
    }

    #[tokio::test]
    async fn safety_finish_reason_is_other() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "candidates": [{"finishReason": "SAFETY"}]
            })))
            .mount(&server)
            .await;

        let response = provider_for(&server)
            .chat(&request(vec![Message::user("hi")]))
            .await
            .unwrap();
        assert_eq!(response.stop_reason, StopReason::Other("SAFETY".to_owned()));
        assert_eq!(response.message.text(), "");
    }

    #[tokio::test]
    async fn blocked_prompt_is_an_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"promptFeedback": {"blockReason": "SAFETY"}})),
            )
            .mount(&server)
            .await;

        let err = provider_for(&server)
            .chat(&request(vec![Message::user("hi")]))
            .await
            .unwrap_err();
        assert!(matches!(err, LlmError::Gemini(crate::gemini::GeminiError::Blocked { .. })));
    }

    #[tokio::test]
    async fn non_success_status_is_an_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(429).set_body_string("slow down"))
            .mount(&server)
            .await;

        let err = provider_for(&server)
            .chat(&request(vec![Message::user("hi")]))
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            LlmError::Gemini(crate::gemini::GeminiError::Status { status: 429, .. })
        ));
    }
}
