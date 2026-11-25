pub mod batch;
pub mod client;
pub mod config;
pub mod image;

use thiserror::Error;

pub use client::Client;
pub use config::GeminiConfig;

#[derive(Debug, Error)]
pub enum GeminiError {
    #[error("no Gemini API key: set [gemini].api_key in config.toml or the GEMINI_API_KEY environment variable")]
    MissingApiKey,

    #[error("request to {url} failed: {source}")]
    Request {
        url: String,
        #[source]
        source: Box<reqwest::Error>,
    },

    #[error("gemini returned {status} for {url}: {body}")]
    Status { status: u16, url: String, body: String },

    #[error("failed to parse the gemini response from {url}: {source}")]
    Decode {
        url: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("gemini blocked the request: {reason}")]
    Blocked { reason: String },

    #[error("gemini returned no image (finish reason {finish_reason}); it said: {text}")]
    NoImage { finish_reason: String, text: String },

    #[error("failed to decode the image gemini returned: {0}")]
    ImageBase64(#[source] base64::DecodeError),

    #[error("failed to re-encode the image gemini returned as PNG: {0}")]
    ImageReencode(#[source] ::image::ImageError),

    #[error(
        "gemini only supports these aspect ratios: {supported}; {width}x{height} reduces to {ratio}"
    )]
    UnsupportedAspectRatio {
        width: i32,
        height: i32,
        ratio: String,
        supported: &'static str,
    },

    #[error("pass both --width and --height (or neither): gemini takes an aspect ratio, not a single dimension")]
    IncompleteDimensions,

    #[error("gemini batch {name} ended in state {state}: {message}")]
    BatchFailed { name: String, state: String, message: String },

    #[error("gemini batch {name} did not finish within {waited:?}")]
    BatchTimedOut { name: String, waited: std::time::Duration },

    #[error("gemini batch {name} was cancelled by interrupt")]
    BatchInterrupted { name: String },

    #[error("the batch request is {bytes} bytes, over the 20MB inline limit")]
    BatchTooLarge { bytes: usize },

    #[error("gemini created a batch but returned no job name")]
    BatchMissingName,

    #[error("gemini batch item failed: {message}")]
    BatchItemFailed { message: String },

    #[error("every image in the batch failed: {errors}")]
    AllCopiesFailed { errors: String },

    #[error("invalid duration {value:?}: use a number with a unit, e.g. 90s, 30m, 1h, 1h30m")]
    InvalidDuration { value: String },
}
