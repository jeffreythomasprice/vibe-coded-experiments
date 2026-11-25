use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::gemini::batch::RawBatch;
use crate::gemini::{GeminiConfig, GeminiError};

const API_KEY_HEADER: &str = "x-goog-api-key";

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
}

impl Client {
    pub fn new(config: &GeminiConfig) -> Result<Self, GeminiError> {
        let api_key = config.resolve_api_key()?;
        let http = reqwest::Client::builder()
            .timeout(config.timeout())
            .build()
            .expect("reqwest client with a plain timeout is always buildable");
        Ok(Self {
            http,
            base_url: config.base_url.trim_end_matches('/').to_owned(),
            api_key,
        })
    }

    pub async fn generate_content(&self, model: &str, body: &Value) -> Result<RawResponse, GeminiError> {
        let url = format!("{}/models/{model}:generateContent", self.base_url);
        self.send(self.http.post(&url).json(body), url).await
    }

    pub async fn create_batch(&self, model: &str, body: &Value) -> Result<RawBatch, GeminiError> {
        let url = format!("{}/models/{model}:batchGenerateContent", self.base_url);
        self.send(self.http.post(&url).json(body), url).await
    }

    pub async fn get_batch(&self, name: &str) -> Result<RawBatch, GeminiError> {
        let url = format!("{}/{name}", self.base_url);
        self.send(self.http.get(&url), url).await
    }

    pub async fn cancel_batch(&self, name: &str) -> Result<(), GeminiError> {
        let url = format!("{}/{name}:cancel", self.base_url);
        self.send::<Value>(self.http.post(&url), url).await.map(drop)
    }

    async fn send<T: DeserializeOwned>(
        &self,
        request: reqwest::RequestBuilder,
        url: String,
    ) -> Result<T, GeminiError> {
        let response = request
            .header(API_KEY_HEADER, &self.api_key)
            .send()
            .await
            .map_err(|source| GeminiError::Request {
                url: url.clone(),
                source: Box::new(source),
            })?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(GeminiError::Status {
                status: status.as_u16(),
                url,
                body,
            });
        }

        let bytes = response.bytes().await.map_err(|source| GeminiError::Request {
            url: url.clone(),
            source: Box::new(source),
        })?;
        if bytes.is_empty() {
            return serde_json::from_slice(b"{}").map_err(|source| GeminiError::Decode { url, source });
        }
        serde_json::from_slice(&bytes).map_err(|source| GeminiError::Decode { url, source })
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawResponse {
    #[serde(default)]
    pub candidates: Vec<RawCandidate>,
    #[serde(default)]
    pub prompt_feedback: Option<RawPromptFeedback>,
    #[serde(default)]
    pub usage_metadata: Option<RawUsage>,
}

impl RawResponse {
    /// The first candidate, or `Blocked` when the prompt was rejected or no
    /// candidate came back at all.
    pub fn first_candidate(self) -> Result<RawCandidate, GeminiError> {
        let block_reason = self.prompt_feedback.and_then(|feedback| feedback.block_reason);
        match self.candidates.into_iter().next() {
            Some(candidate) => Ok(candidate),
            None => Err(GeminiError::Blocked {
                reason: block_reason.unwrap_or_else(|| "the response had no candidates".to_owned()),
            }),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawPromptFeedback {
    #[serde(default)]
    pub block_reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawUsage {
    #[serde(default)]
    pub prompt_token_count: u32,
    #[serde(default)]
    pub candidates_token_count: u32,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawCandidate {
    #[serde(default)]
    pub content: Option<RawContent>,
    #[serde(default)]
    pub finish_reason: Option<String>,
    #[serde(default)]
    pub logprobs_result: Option<RawLogprobsResult>,
}

impl RawCandidate {
    pub fn parts(&self) -> &[RawPart] {
        self.content.as_ref().map(|c| c.parts.as_slice()).unwrap_or(&[])
    }

    /// Visible text only: thought summaries are excluded.
    pub fn text(&self) -> String {
        self.parts()
            .iter()
            .filter(|part| !part.thought)
            .filter_map(|part| part.text.as_deref())
            .collect::<Vec<_>>()
            .join("")
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct RawContent {
    #[serde(default)]
    pub parts: Vec<RawPart>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawPart {
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub thought: bool,
    #[serde(default)]
    pub inline_data: Option<RawInlineData>,
    #[serde(default)]
    pub function_call: Option<RawFunctionCall>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawInlineData {
    #[serde(default)]
    pub mime_type: String,
    pub data: String,
}

#[derive(Debug, Deserialize)]
pub struct RawFunctionCall {
    pub name: String,
    #[serde(default)]
    pub args: Value,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawLogprobsResult {
    #[serde(default)]
    pub chosen_candidates: Vec<RawLogprobToken>,
    #[serde(default)]
    pub top_candidates: Vec<RawTopCandidates>,
}

#[derive(Debug, Default, Deserialize)]
pub struct RawTopCandidates {
    #[serde(default)]
    pub candidates: Vec<RawLogprobToken>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawLogprobToken {
    pub token: String,
    #[serde(default)]
    pub log_probability: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn client_for(server: &MockServer) -> Client {
        let config = GeminiConfig {
            api_key: Some("secret-key".to_owned()),
            base_url: server.uri(),
            timeout_secs: 5,
            ..Default::default()
        };
        Client::new(&config).unwrap()
    }

    #[tokio::test]
    async fn sends_key_in_header_not_url() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/models/m1:generateContent"))
            .and(header("x-goog-api-key", "secret-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "candidates": [{"content": {"parts": [{"text": "hi"}]}, "finishReason": "STOP"}]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let response = client_for(&server).generate_content("m1", &json!({})).await.unwrap();
        assert_eq!(response.first_candidate().unwrap().text(), "hi");

        let requests = server.received_requests().await.unwrap();
        assert!(!requests[0].url.to_string().contains("secret-key"));
    }

    #[tokio::test]
    async fn non_success_is_a_status_error_without_the_key() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(403).set_body_string("denied"))
            .mount(&server)
            .await;

        let err = client_for(&server)
            .generate_content("m1", &json!({}))
            .await
            .unwrap_err();
        match err {
            GeminiError::Status { status, url, body } => {
                assert_eq!(status, 403);
                assert_eq!(body, "denied");
                assert!(!url.contains("secret-key"));
            }
            other => panic!("expected Status, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn malformed_json_is_a_decode_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .mount(&server)
            .await;

        let err = client_for(&server)
            .generate_content("m1", &json!({}))
            .await
            .unwrap_err();
        assert!(matches!(err, GeminiError::Decode { .. }));
    }

    #[test]
    fn missing_key_fails_construction() {
        let config = GeminiConfig {
            api_key: None,
            ..Default::default()
        };
        if std::env::var(crate::gemini::config::API_KEY_ENV).is_err() {
            assert!(matches!(Client::new(&config), Err(GeminiError::MissingApiKey)));
        }
    }

    #[test]
    fn no_candidates_with_block_reason_is_blocked() {
        let raw: RawResponse =
            serde_json::from_value(json!({"promptFeedback": {"blockReason": "SAFETY"}})).unwrap();
        match raw.first_candidate() {
            Err(GeminiError::Blocked { reason }) => assert_eq!(reason, "SAFETY"),
            other => panic!("expected Blocked, got {other:?}"),
        }
    }

    #[test]
    fn thought_text_is_excluded() {
        let raw: RawResponse = serde_json::from_value(json!({
            "candidates": [{"content": {"parts": [
                {"text": "thinking", "thought": true},
                {"text": "answer"}
            ]}}]
        }))
        .unwrap();
        assert_eq!(raw.first_candidate().unwrap().text(), "answer");
    }
}
