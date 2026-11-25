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
}
