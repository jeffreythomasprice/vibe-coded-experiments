use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::{Deserialize, Serialize};

use crate::sd::SdError;

#[derive(Debug, Clone, Serialize, Default)]
pub struct Guidance {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub txt_cfg: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distilled_guidance: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct SampleParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_steps: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guidance: Option<Guidance>,
}

/// A request to `POST /sdcpp/v1/img_gen`. Only carries the fields this project
/// actually sets; every other field in the server's schema (LoRA, hires,
/// control/IP-adapter images, ...) is left to the server's own defaults.
#[derive(Debug, Clone, Serialize, Default)]
pub struct ImgGenRequest {
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub negative_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clip_skip: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    /// Always pinned to 1: `--copies N` is N sequential requests with distinct
    /// seeds, not one server-side batch, so progress and failures are visible
    /// and isolated per copy.
    pub batch_count: i32,
    /// Base64-encoded reference images for in-context conditioning.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ref_images: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_params: Option<SampleParams>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_format: Option<String>,
    pub embed_image_metadata: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SubmitResponse {
    pub id: String,
    pub status: String,
    pub poll_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Job {
    pub id: String,
    pub status: String,
    #[serde(default)]
    pub queue_position: i64,
    #[serde(default)]
    pub result: Option<JobResult>,
    #[serde(default)]
    pub error: Option<JobError>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JobResult {
    pub images: Vec<JobImage>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JobImage {
    pub b64_json: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JobError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Capabilities {
    pub model: ModelInfo,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModelInfo {
    pub path: String,
}

/// A thin async client for `sd-server`'s native `/sdcpp/v1/...` API. Holds no
/// process-lifecycle state of its own (see `sd::daemon` for that) — just a base
/// URL and an HTTP client.
#[derive(Debug)]
pub struct Client {
    http: reqwest::Client,
    base_url: String,
}

impl Client {
    pub fn new(base_url: impl Into<String>, request_timeout: Duration) -> Self {
        let http = reqwest::Client::builder()
            .timeout(request_timeout)
            .build()
            .expect("reqwest client with a plain timeout is always buildable");
        Self {
            http,
            base_url: base_url.into(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, SdError> {
        let url = self.url(path);
        let response = self.http.get(&url).send().await.map_err(|source| SdError::Request {
            url: url.clone(),
            source: Box::new(source),
        })?;
        Self::decode(url, response).await
    }

    async fn decode<T: serde::de::DeserializeOwned>(url: String, response: reqwest::Response) -> Result<T, SdError> {
        let status = response.status();
        let body = response.text().await.map_err(|source| SdError::Request {
            url: url.clone(),
            source: Box::new(source),
        })?;
        if !status.is_success() {
            return Err(SdError::Status {
                status: status.as_u16(),
                url,
                body,
            });
        }
        serde_json::from_str(&body).map_err(|source| SdError::Decode { url, source })
    }

    /// `GET /sdcpp/v1/capabilities`. Doubles as a health check: any 200 response
    /// with a parseable body means the process at `base_url` is a live sd-server.
    pub async fn capabilities(&self) -> Result<Capabilities, SdError> {
        self.get_json("/sdcpp/v1/capabilities").await
    }

    /// `POST /sdcpp/v1/img_gen`. Submission only — call `job`/`wait_for_images`
    /// to observe progress and collect the result.
    pub async fn submit_img_gen(&self, request: &ImgGenRequest) -> Result<SubmitResponse, SdError> {
        let url = self.url("/sdcpp/v1/img_gen");
        let response = self
            .http
            .post(&url)
            .json(request)
            .send()
            .await
            .map_err(|source| SdError::Request {
                url: url.clone(),
                source: Box::new(source),
            })?;
        Self::decode(url, response).await
    }

    /// `GET /sdcpp/v1/jobs/{id}`. A `410 Gone` (the server pruned this job's
    /// result) is reported as `SdError::JobGone` rather than the generic
    /// `Status` error, since it is a distinct, non-retryable outcome.
    pub async fn job(&self, id: &str) -> Result<Job, SdError> {
        let path = format!("/sdcpp/v1/jobs/{id}");
        let url = self.url(&path);
        let response = self.http.get(&url).send().await.map_err(|source| SdError::Request {
            url: url.clone(),
            source: Box::new(source),
        })?;
        if response.status().as_u16() == 410 {
            return Err(SdError::JobGone { id: id.to_owned() });
        }
        Self::decode(url, response).await
    }

    /// `POST /sdcpp/v1/jobs/{id}/cancel`.
    pub async fn cancel(&self, id: &str) -> Result<Job, SdError> {
        let path = format!("/sdcpp/v1/jobs/{id}/cancel");
        let url = self.url(&path);
        let response = self.http.post(&url).send().await.map_err(|source| SdError::Request {
            url: url.clone(),
            source: Box::new(source),
        })?;
        Self::decode(url, response).await
    }

    /// Polls `job(id)` until it reaches a terminal state, returning the
    /// base64-decoded image bytes on success. `failed`/`cancelled` both surface
    /// as `SdError::JobFailed`; a `queued`/`generating` job still running when
    /// `timeout` elapses is `SdError::JobTimeout`.
    pub async fn wait_for_images(
        &self,
        id: &str,
        poll_interval: Duration,
        timeout: Duration,
    ) -> Result<Vec<Vec<u8>>, SdError> {
        let deadline = Instant::now() + timeout;
        loop {
            let job = self.job(id).await?;
            match job.status.as_str() {
                "completed" => return Self::decode_images(&job),
                "failed" | "cancelled" => return Err(Self::job_error(job)),
                _ => {
                    if Instant::now() >= deadline {
                        return Err(SdError::JobTimeout {
                            id: id.to_owned(),
                            waited: timeout,
                        });
                    }
                    tokio::time::sleep(poll_interval).await;
                }
            }
        }
    }

    fn decode_images(job: &Job) -> Result<Vec<Vec<u8>>, SdError> {
        let Some(result) = &job.result else {
            return Err(SdError::JobFailed {
                id: job.id.clone(),
                code: "completed_without_result".to_owned(),
                message: "job reported completed but result was null".to_owned(),
            });
        };
        result
            .images
            .iter()
            .enumerate()
            .map(|(index, image)| {
                BASE64
                    .decode(&image.b64_json)
                    .map_err(|source| SdError::ImageDecode { index, source })
            })
            .collect()
    }

    fn job_error(job: Job) -> SdError {
        let (code, message) = match job.error {
            Some(error) => (error.code, error.message),
            None => (job.status.clone(), "no error detail provided".to_owned()),
        };
        SdError::JobFailed {
            id: job.id,
            code,
            message,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    fn client(server: &MockServer) -> Client {
        Client::new(server.uri(), Duration::from_secs(5))
    }

    #[tokio::test]
    async fn capabilities_reports_the_loaded_model_path() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/sdcpp/v1/capabilities"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "model": {"name": "qwen", "stem": "qwen_image_2.1", "path": "/models/qwen.gguf"},
                "current_mode": "img_gen",
            })))
            .mount(&server)
            .await;

        let caps = client(&server).capabilities().await.unwrap();
        assert_eq!(caps.model.path, "/models/qwen.gguf");
    }

    #[tokio::test]
    async fn capabilities_on_connection_refused_is_a_request_error() {
        // Port 1 is a reserved low port nothing binds to in this sandbox.
        let client = Client::new("http://127.0.0.1:1", Duration::from_millis(200));
        let err = client.capabilities().await.unwrap_err();
        assert!(matches!(err, SdError::Request { .. }));
    }

    #[tokio::test]
    async fn submit_img_gen_returns_the_poll_url() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/sdcpp/v1/img_gen"))
            .respond_with(ResponseTemplate::new(202).set_body_json(json!({
                "id": "job_1",
                "kind": "img_gen",
                "status": "queued",
                "created": 1,
                "poll_url": "/sdcpp/v1/jobs/job_1",
            })))
            .mount(&server)
            .await;

        let request = ImgGenRequest {
            prompt: "a cat".to_owned(),
            batch_count: 1,
            embed_image_metadata: true,
            ..Default::default()
        };
        let submitted = client(&server).submit_img_gen(&request).await.unwrap();
        assert_eq!(submitted.id, "job_1");
        assert_eq!(submitted.poll_url, "/sdcpp/v1/jobs/job_1");
    }

    #[tokio::test]
    async fn submit_img_gen_400_is_a_status_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/sdcpp/v1/img_gen"))
            .respond_with(ResponseTemplate::new(400).set_body_string("invalid generation parameters"))
            .mount(&server)
            .await;

        let request = ImgGenRequest {
            prompt: "a cat".to_owned(),
            batch_count: 1,
            ..Default::default()
        };
        let err = client(&server).submit_img_gen(&request).await.unwrap_err();
        assert!(matches!(err, SdError::Status { status: 400, .. }));
    }

    #[tokio::test]
    async fn job_gone_is_reported_distinctly() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/sdcpp/v1/jobs/job_1"))
            .respond_with(ResponseTemplate::new(410))
            .mount(&server)
            .await;

        let err = client(&server).job("job_1").await.unwrap_err();
        assert!(matches!(err, SdError::JobGone { id } if id == "job_1"));
    }

    #[tokio::test]
    async fn cancel_returns_the_cancelled_job() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/sdcpp/v1/jobs/job_1/cancel"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "job_1",
                "status": "cancelled",
                "created": 1,
            })))
            .mount(&server)
            .await;

        let job = client(&server).cancel("job_1").await.unwrap();
        assert_eq!(job.status, "cancelled");
    }

    #[tokio::test]
    async fn wait_for_images_polls_through_queued_and_generating_to_completed() {
        let server = MockServer::start().await;
        let responses = [
            json!({"id": "job_1", "status": "queued", "created": 1}),
            json!({"id": "job_1", "status": "generating", "created": 1}),
            json!({
                "id": "job_1",
                "status": "completed",
                "created": 1,
                "result": {"output_format": "png", "images": [{"index": 0, "b64_json": BASE64.encode(b"fake png bytes")}]},
            }),
        ];
        for response in responses {
            Mock::given(method("GET"))
                .and(path("/sdcpp/v1/jobs/job_1"))
                .respond_with(ResponseTemplate::new(200).set_body_json(response))
                .up_to_n_times(1)
                .mount(&server)
                .await;
        }

        let images = client(&server)
            .wait_for_images("job_1", Duration::from_millis(1), Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(images, vec![b"fake png bytes".to_vec()]);
    }

    #[tokio::test]
    async fn wait_for_images_surfaces_a_failed_job() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/sdcpp/v1/jobs/job_1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "job_1",
                "status": "failed",
                "created": 1,
                "error": {"code": "generation_failed", "message": "generate_image returned empty results"},
            })))
            .mount(&server)
            .await;

        let err = client(&server)
            .wait_for_images("job_1", Duration::from_millis(1), Duration::from_secs(5))
            .await
            .unwrap_err();
        match err {
            SdError::JobFailed { code, message, .. } => {
                assert_eq!(code, "generation_failed");
                assert_eq!(message, "generate_image returned empty results");
            }
            other => panic!("expected JobFailed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn wait_for_images_times_out_on_a_stuck_job() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/sdcpp/v1/jobs/job_1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "job_1",
                "status": "generating",
                "created": 1,
            })))
            .mount(&server)
            .await;

        let err = client(&server)
            .wait_for_images("job_1", Duration::from_millis(1), Duration::from_millis(20))
            .await
            .unwrap_err();
        assert!(matches!(err, SdError::JobTimeout { .. }));
    }

    #[test]
    fn img_gen_request_serializes_only_set_fields() {
        let request = ImgGenRequest {
            prompt: "a cat".to_owned(),
            batch_count: 1,
            embed_image_metadata: true,
            sample_params: Some(SampleParams {
                sample_steps: Some(20),
                guidance: Some(Guidance {
                    txt_cfg: Some(6.0),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        let value = serde_json::to_value(&request).unwrap();
        assert_eq!(value["prompt"], "a cat");
        assert_eq!(value["batch_count"], 1);
        assert_eq!(value["sample_params"]["sample_steps"], 20);
        assert_eq!(value["sample_params"]["guidance"]["txt_cfg"], 6.0);
        assert!(value.get("negative_prompt").is_none());
        assert!(value.get("ref_images").is_none());
        assert!(value["sample_params"]["guidance"].get("distilled_guidance").is_none());
    }
}
