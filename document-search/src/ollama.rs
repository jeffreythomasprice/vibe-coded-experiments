use serde::{Deserialize, Serialize};
use tokio::sync::watch;

#[derive(thiserror::Error, Debug)]
pub enum OllamaError {
    #[error("http request to {url}: {source}")]
    Http {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    #[error("ollama returned status {status} for {url}: {body}")]
    Status {
        url: String,
        status: u16,
        body: String,
    },

    #[error("decoding response from {url}: {source}")]
    Decode {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    #[error("ollama returned an empty embedding for model {model:?}")]
    EmptyEmbedding { model: String },

    #[error("embedding request cancelled")]
    Cancelled,
}

#[derive(Serialize)]
struct EmbedRequest<'a> {
    model: &'a str,
    prompt: &'a str,
}

#[derive(Deserialize)]
struct EmbedResponse {
    embedding: Vec<f32>,
}

/// POST `<url>/api/embeddings` with `{"model": ..., "prompt": ...}` and return
/// the resulting vector. Uses the legacy single-input endpoint, which all
/// versions of Ollama with embedding support accept.
///
/// If `cancel` is provided and fires before the HTTP call finishes, the request
/// future is dropped (reqwest aborts the underlying connection) and
/// `OllamaError::Cancelled` is returned.
pub async fn embed(
    client: &reqwest::Client,
    url: &str,
    model: &str,
    text: &str,
    cancel: Option<&watch::Receiver<bool>>,
) -> Result<Vec<f32>, OllamaError> {
    if let Some(rx) = cancel {
        if *rx.borrow() {
            return Err(OllamaError::Cancelled);
        }
    }
    let fut = embed_inner(client, url, model, text);
    match cancel {
        Some(rx) => {
            let mut rx = rx.clone();
            tokio::select! {
                res = fut => res,
                changed = rx.changed() => {
                    if changed.is_ok() && *rx.borrow() {
                        Err(OllamaError::Cancelled)
                    } else {
                        // Sender dropped without a cancel signal — finish the
                        // call uncancellable rather than failing spuriously.
                        embed_inner(client, url, model, text).await
                    }
                }
            }
        }
        None => fut.await,
    }
}

async fn embed_inner(
    client: &reqwest::Client,
    url: &str,
    model: &str,
    text: &str,
) -> Result<Vec<f32>, OllamaError> {
    let endpoint = format!("{}/api/embeddings", url.trim_end_matches('/'));
    let resp = client
        .post(&endpoint)
        .json(&EmbedRequest { model, prompt: text })
        .send()
        .await
        .map_err(|source| OllamaError::Http {
            url: endpoint.clone(),
            source,
        })?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(OllamaError::Status {
            url: endpoint,
            status: status.as_u16(),
            body,
        });
    }

    let parsed: EmbedResponse = resp.json().await.map_err(|source| OllamaError::Decode {
        url: endpoint,
        source,
    })?;
    if parsed.embedding.is_empty() {
        return Err(OllamaError::EmptyEmbedding {
            model: model.to_string(),
        });
    }
    Ok(parsed.embedding)
}

/// Issue a single embed call to learn the model's vector length. Used at
/// startup to fill the `embedding_model_dimensions` cache when there's no
/// row for `model` yet.
pub async fn probe_dimensions(
    client: &reqwest::Client,
    url: &str,
    model: &str,
) -> Result<usize, OllamaError> {
    let v = embed(client, url, model, " ", None).await?;
    Ok(v.len())
}
