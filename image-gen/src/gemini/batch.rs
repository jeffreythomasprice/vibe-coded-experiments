use std::str::FromStr;
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::{Value, json};

use crate::gemini::client::RawResponse;
use crate::gemini::image::png_from_response;
use crate::gemini::{Client, GeminiError};

pub const MAX_INLINE_BYTES: usize = 20 * 1024 * 1024;
pub const DEFAULT_WAIT: Duration = Duration::from_secs(60 * 60);
const MAX_CONSECUTIVE_POLL_FAILURES: u32 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatchWait(pub Duration);

impl FromStr for BatchWait {
    type Err = GeminiError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        parse_duration(input).map(BatchWait)
    }
}

fn parse_duration(input: &str) -> Result<Duration, GeminiError> {
    let invalid = || GeminiError::InvalidDuration {
        value: input.to_owned(),
    };
    let mut rest = input.trim();
    if rest.is_empty() {
        return Err(invalid());
    }
    let mut total = 0u64;
    while !rest.is_empty() {
        let digits = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
        if digits == 0 {
            return Err(invalid());
        }
        let amount: u64 = rest[..digits].parse().map_err(|_| invalid())?;
        let multiplier = match rest[digits..].chars().next() {
            Some('s') => 1,
            Some('m') => 60,
            Some('h') => 3600,
            _ => return Err(invalid()),
        };
        total = amount
            .checked_mul(multiplier)
            .and_then(|seconds| total.checked_add(seconds))
            .ok_or_else(invalid)?;
        rest = &rest[digits + 1..];
    }
    if total == 0 {
        return Err(invalid());
    }
    Ok(Duration::from_secs(total))
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawBatch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub metadata: Option<RawBatchMetadata>,
    #[serde(default)]
    pub dest: Option<RawBatchOutput>,
    #[serde(default)]
    pub response: Option<RawBatchOutput>,
    #[serde(default)]
    pub error: Option<Value>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawBatchMetadata {
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub output: Option<RawBatchOutput>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawBatchOutput {
    #[serde(default)]
    pub inlined_responses: Option<InlinedResponses>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum InlinedResponses {
    List(Vec<RawInlined>),
    Wrapped(WrappedInlined),
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WrappedInlined {
    #[serde(default)]
    pub inlined_responses: Vec<RawInlined>,
}

#[derive(Debug, Default, Deserialize)]
pub struct RawInlined {
    #[serde(default)]
    pub response: Option<RawResponse>,
    #[serde(default)]
    pub error: Option<Value>,
    #[serde(default)]
    pub metadata: Option<RawInlinedMetadata>,
}

#[derive(Debug, Default, Deserialize)]
pub struct RawInlinedMetadata {
    #[serde(default)]
    pub key: Option<String>,
}

impl RawBatch {
    /// The docs show the state at both `.state` and `.metadata.state`, with
    /// either a `JOB_STATE_` or `BATCH_STATE_` prefix.
    pub fn state_name(&self) -> String {
        let raw = self
            .state
            .as_deref()
            .or_else(|| self.metadata.as_ref().and_then(|m| m.state.as_deref()))
            .unwrap_or("UNKNOWN");
        raw.rsplit("STATE_").next().unwrap_or(raw).to_owned()
    }

    pub fn error_message(&self) -> String {
        match &self.error {
            Some(error) => error
                .get("message")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or_else(|| error.to_string()),
            None => "no error detail".to_owned(),
        }
    }

    pub fn into_inlined(self) -> Vec<RawInlined> {
        let outputs = [
            self.dest,
            self.response,
            self.metadata.and_then(|metadata| metadata.output),
        ];
        for output in outputs.into_iter().flatten() {
            match output.inlined_responses {
                Some(InlinedResponses::List(list)) => return list,
                Some(InlinedResponses::Wrapped(wrapped)) => return wrapped.inlined_responses,
                None => {}
            }
        }
        Vec::new()
    }
}

#[derive(Debug)]
pub struct CopyResult {
    pub index: u32,
    pub result: Result<Vec<u8>, GeminiError>,
}

#[derive(Debug)]
pub enum BatchOutcome {
    Done(Vec<CopyResult>),
    /// The job was cancelled at the deadline; `partial` is whatever it had
    /// already produced.
    TimedOut { name: String, partial: Vec<CopyResult> },
    Unavailable { bytes: usize },
}

pub fn create_body(display_name: &str, items: &[(u32, Value)]) -> Value {
    let requests: Vec<Value> = items
        .iter()
        .map(|(index, request)| json!({ "request": request, "metadata": { "key": index.to_string() } }))
        .collect();
    json!({
        "batch": {
            "display_name": display_name,
            "input_config": { "requests": { "requests": requests } },
        }
    })
}

/// Results are matched by `metadata.key` when echoed back, else by position.
fn collect(inlined: Vec<RawInlined>, indices: &[u32]) -> Vec<CopyResult> {
    inlined
        .into_iter()
        .enumerate()
        .filter_map(|(position, item)| {
            let keyed = item
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.key.as_deref())
                .and_then(|key| key.parse::<u32>().ok())
                .filter(|key| indices.contains(key));
            let index = keyed.or_else(|| indices.get(position).copied())?;
            let result = match (item.response, item.error) {
                (Some(response), _) => png_from_response(response),
                (None, Some(error)) => Err(GeminiError::BatchItemFailed {
                    message: error
                        .get("message")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                        .unwrap_or_else(|| error.to_string()),
                }),
                (None, None) => Err(GeminiError::BatchItemFailed {
                    message: "the batch returned an empty entry".to_owned(),
                }),
            };
            Some(CopyResult { index, result })
        })
        .collect()
}

fn fill_missing(mut results: Vec<CopyResult>, indices: &[u32]) -> Vec<CopyResult> {
    for index in indices {
        if !results.iter().any(|result| result.index == *index) {
            results.push(CopyResult {
                index: *index,
                result: Err(GeminiError::BatchItemFailed {
                    message: "the batch returned no result for this request".to_owned(),
                }),
            });
        }
    }
    results.sort_by_key(|result| result.index);
    results
}

async fn cancel_quietly(client: &Client, name: &str) {
    if let Err(error) = client.cancel_batch(name).await {
        tracing::warn!(batch = name, %error, "failed to cancel the batch");
    }
}

async fn cancel_and_collect(client: &Client, name: &str, indices: &[u32]) -> Vec<CopyResult> {
    cancel_quietly(client, name).await;
    match client.get_batch(name).await {
        Ok(batch) => collect(batch.into_inlined(), indices),
        Err(error) => {
            tracing::warn!(batch = name, %error, "failed to read the cancelled batch");
            Vec::new()
        }
    }
}

/// Submits `items` as one inline batch and blocks until it ends, `wait`
/// elapses, or the process is interrupted. Creating a batch is not idempotent,
/// so the create call is never retried.
pub async fn run_batch(
    client: &Client,
    model: &str,
    items: Vec<(u32, Value)>,
    wait: Duration,
    poll: Duration,
) -> Result<BatchOutcome, GeminiError> {
    let body = create_body("image-gen", &items);
    let bytes = body.to_string().len();
    if bytes > MAX_INLINE_BYTES {
        return Ok(BatchOutcome::Unavailable { bytes });
    }

    let indices: Vec<u32> = items.iter().map(|(index, _)| *index).collect();
    let name = client
        .create_batch(model, &body)
        .await?
        .name
        .ok_or(GeminiError::BatchMissingName)?;
    tracing::info!(batch = %name, requests = indices.len(), "submitted batch");

    let started = Instant::now();
    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);
    let mut last_state = String::new();
    let mut failures = 0u32;

    loop {
        match client.get_batch(&name).await {
            Ok(batch) => {
                failures = 0;
                let state = batch.state_name();
                if state != last_state {
                    tracing::info!(
                        batch = %name,
                        %state,
                        elapsed_secs = started.elapsed().as_secs(),
                        "batch state"
                    );
                    last_state = state.clone();
                }
                match state.as_str() {
                    "SUCCEEDED" => {
                        let results = collect(batch.into_inlined(), &indices);
                        return Ok(BatchOutcome::Done(fill_missing(results, &indices)));
                    }
                    "FAILED" | "EXPIRED" | "CANCELLED" => {
                        return Err(GeminiError::BatchFailed {
                            message: batch.error_message(),
                            name,
                            state,
                        });
                    }
                    _ => {}
                }
            }
            Err(error) => {
                failures += 1;
                tracing::warn!(batch = %name, %error, failures, "failed to poll the batch");
                if failures >= MAX_CONSECUTIVE_POLL_FAILURES {
                    cancel_quietly(client, &name).await;
                    return Err(error);
                }
            }
        }

        let elapsed = started.elapsed();
        if elapsed >= wait {
            let partial = cancel_and_collect(client, &name, &indices).await;
            return Ok(BatchOutcome::TimedOut { name, partial });
        }
        tokio::select! {
            _ = tokio::time::sleep(poll.min(wait - elapsed)) => {}
            _ = &mut ctrl_c => {
                cancel_quietly(client, &name).await;
                return Err(GeminiError::BatchInterrupted { name });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gemini::GeminiConfig;
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD as BASE64;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn client_for(server: &MockServer) -> Client {
        Client::new(&GeminiConfig {
            api_key: Some("k".to_owned()),
            base_url: server.uri(),
            timeout_secs: 5,
            batch_poll_secs: 1,
        })
        .unwrap()
    }

    fn items(count: u32) -> Vec<(u32, Value)> {
        (0..count).map(|i| (i, json!({"contents": []}))).collect()
    }

    fn image_response(bytes: &[u8]) -> Value {
        json!({"candidates": [{
            "content": {"parts": [{"inlineData": {"mimeType": "image/png", "data": BASE64.encode(bytes)}}]},
            "finishReason": "STOP"
        }]})
    }

    async fn mount_create(server: &MockServer) {
        Mock::given(method("POST"))
            .and(path("/models/m:batchGenerateContent"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"name": "batches/1"})))
            .expect(1)
            .mount(server)
            .await;
    }

    async fn mount_poll(server: &MockServer, body: Value, times: Option<u64>) {
        let mock = Mock::given(method("GET"))
            .and(path("/batches/1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body));
        match times {
            Some(n) => mock.up_to_n_times(n).mount(server).await,
            None => mock.mount(server).await,
        }
    }

    async fn mount_cancel(server: &MockServer, expected: u64) {
        Mock::given(method("POST"))
            .and(path("/batches/1:cancel"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .expect(expected)
            .mount(server)
            .await;
    }

    const FAST: Duration = Duration::from_millis(5);

    #[test]
    fn durations_parse_with_units() {
        assert_eq!(parse_duration("90s").unwrap(), Duration::from_secs(90));
        assert_eq!(parse_duration("30m").unwrap(), Duration::from_secs(1800));
        assert_eq!(parse_duration("1h").unwrap(), Duration::from_secs(3600));
        assert_eq!(parse_duration("1h30m").unwrap(), Duration::from_secs(5400));
    }

    #[test]
    fn bad_durations_are_rejected() {
        for bad in ["", "10", "m", "1x", "0s", "1h-", "abc"] {
            assert!(
                matches!(parse_duration(bad), Err(GeminiError::InvalidDuration { .. })),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn create_body_keys_each_request_by_index() {
        let body = create_body("n", &[(3, json!({"a": 1}))]);
        let entry = &body["batch"]["input_config"]["requests"]["requests"][0];
        assert_eq!(entry["metadata"]["key"], "3");
        assert_eq!(entry["request"]["a"], 1);
    }

    #[test]
    fn state_is_read_from_either_location_and_prefix() {
        let top: RawBatch = serde_json::from_value(json!({"state": "JOB_STATE_RUNNING"})).unwrap();
        assert_eq!(top.state_name(), "RUNNING");
        let nested: RawBatch =
            serde_json::from_value(json!({"metadata": {"state": "BATCH_STATE_SUCCEEDED"}})).unwrap();
        assert_eq!(nested.state_name(), "SUCCEEDED");
    }

    #[test]
    fn inlined_responses_are_found_at_every_documented_location() {
        let entry = json!({"response": image_response(b"x")});
        for value in [
            json!({"dest": {"inlinedResponses": [entry]}}),
            json!({"response": {"inlinedResponses": [entry]}}),
            json!({"response": {"inlinedResponses": {"inlinedResponses": [entry]}}}),
            json!({"metadata": {"output": {"inlinedResponses": {"inlinedResponses": [entry]}}}}),
        ] {
            let batch: RawBatch = serde_json::from_value(value).unwrap();
            assert_eq!(batch.into_inlined().len(), 1);
        }
    }

    fn fixture(name: &str) -> RawBatch {
        let text = std::fs::read_to_string(format!("{}/tests/fixtures/{name}.json", env!("CARGO_MANIFEST_DIR")))
            .unwrap();
        serde_json::from_str(&text).unwrap()
    }

    #[test]
    fn real_create_response_is_pending_with_a_name() {
        let batch = fixture("gemini_batch_create");
        assert!(batch.name.as_deref().unwrap().starts_with("batches/"));
        assert_eq!(batch.state_name(), "PENDING");
    }

    #[test]
    fn real_succeeded_response_yields_keyed_images() {
        let batch = fixture("gemini_batch_succeeded");
        assert_eq!(batch.state_name(), "SUCCEEDED");
        let results = collect(batch.into_inlined(), &[0, 1]);
        assert_eq!(results.len(), 2);
        assert_eq!(results[1].index, 1);
        assert_eq!(results[0].result.as_ref().unwrap(), b"fake-png-bytes");
    }

    #[test]
    fn real_cancelled_response_has_no_results() {
        let batch = fixture("gemini_batch_cancelled");
        assert_eq!(batch.state_name(), "CANCELLED");
        assert!(batch.into_inlined().is_empty());
    }

    #[tokio::test]
    async fn succeeds_after_polling_and_orders_by_key() {
        let server = MockServer::start().await;
        mount_create(&server).await;
        mount_poll(&server, json!({"state": "JOB_STATE_PENDING"}), Some(1)).await;
        mount_poll(
            &server,
            json!({"state": "JOB_STATE_SUCCEEDED", "dest": {"inlinedResponses": [
                {"metadata": {"key": "1"}, "response": image_response(b"one")},
                {"metadata": {"key": "0"}, "response": image_response(b"zero")},
            ]}}),
            None,
        )
        .await;

        let outcome = run_batch(&client_for(&server), "m", items(2), Duration::from_secs(5), FAST)
            .await
            .unwrap();
        let BatchOutcome::Done(results) = outcome else { panic!("expected Done") };
        assert_eq!(results.len(), 2);
        let by_index = |i: u32| {
            results
                .iter()
                .find(|r| r.index == i)
                .unwrap()
                .result
                .as_ref()
                .unwrap()
                .clone()
        };
        assert_eq!(by_index(0), b"zero");
        assert_eq!(by_index(1), b"one");
    }

    #[tokio::test]
    async fn a_failed_item_does_not_sink_the_others() {
        let server = MockServer::start().await;
        mount_create(&server).await;
        mount_poll(
            &server,
            json!({"state": "JOB_STATE_SUCCEEDED", "dest": {"inlinedResponses": [
                {"response": image_response(b"ok")},
                {"error": {"code": 400, "message": "blocked"}},
            ]}}),
            None,
        )
        .await;

        let outcome = run_batch(&client_for(&server), "m", items(3), Duration::from_secs(5), FAST)
            .await
            .unwrap();
        let BatchOutcome::Done(results) = outcome else { panic!("expected Done") };
        assert!(results[0].result.is_ok());
        assert!(matches!(
            &results[1].result,
            Err(GeminiError::BatchItemFailed { message }) if message == "blocked"
        ));
        assert!(matches!(&results[2].result, Err(GeminiError::BatchItemFailed { .. })));
    }

    #[tokio::test]
    async fn failed_and_expired_jobs_are_errors() {
        for state in ["JOB_STATE_FAILED", "JOB_STATE_EXPIRED"] {
            let server = MockServer::start().await;
            mount_create(&server).await;
            mount_poll(&server, json!({"state": state, "error": {"message": "boom"}}), None).await;

            let err = run_batch(&client_for(&server), "m", items(1), Duration::from_secs(5), FAST)
                .await
                .unwrap_err();
            match err {
                GeminiError::BatchFailed { state: got, message, .. } => {
                    assert_eq!(format!("JOB_STATE_{got}"), state);
                    assert_eq!(message, "boom");
                }
                other => panic!("expected BatchFailed, got {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn timeout_cancels_and_keeps_partial_results() {
        let server = MockServer::start().await;
        mount_create(&server).await;
        mount_cancel(&server, 1).await;
        mount_poll(
            &server,
            json!({"state": "JOB_STATE_RUNNING", "dest": {"inlinedResponses": [
                {"metadata": {"key": "1"}, "response": image_response(b"done")},
            ]}}),
            None,
        )
        .await;

        let outcome = run_batch(
            &client_for(&server),
            "m",
            items(2),
            Duration::from_millis(30),
            FAST,
        )
        .await
        .unwrap();
        let BatchOutcome::TimedOut { name, partial } = outcome else { panic!("expected TimedOut") };
        assert_eq!(name, "batches/1");
        assert_eq!(partial.len(), 1);
        assert_eq!(partial[0].index, 1);
    }

    #[tokio::test]
    async fn oversized_request_is_unavailable_without_calling_the_api() {
        let server = MockServer::start().await;
        let big = "x".repeat(MAX_INLINE_BYTES + 1);
        let outcome = run_batch(
            &client_for(&server),
            "m",
            vec![(0, json!({"text": big}))],
            Duration::from_secs(1),
            FAST,
        )
        .await
        .unwrap();
        assert!(matches!(outcome, BatchOutcome::Unavailable { .. }));
        assert!(server.received_requests().await.unwrap().is_empty());
    }
}
