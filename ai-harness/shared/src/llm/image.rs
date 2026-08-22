//! Image types: input sources and generated-image output.

use serde::{Deserialize, Serialize};

/// An image attached to a `ContentBlock::Image`.
///
/// `data` is always base64 — never raw bytes. This crosses
/// `serde_wasm_bindgen` into the Leptos frontend, where a `Vec<u8>` becomes a
/// JS array of numbers (large and slow to marshal); every provider this crate
/// talks to wants base64 on the wire anyway, so this avoids a decode/encode
/// round trip in both directions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageSource {
    pub media_type: MediaType,
    pub data: String,
}

/// An image MIME type.
///
/// Serializes as the bare MIME string (`"image/png"`, ...). `Other` preserves
/// whatever string a provider actually sent rather than failing to parse —
/// see the same pattern on `StopReason`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaType {
    #[serde(rename = "image/png")]
    Png,
    #[serde(rename = "image/jpeg")]
    Jpeg,
    #[serde(rename = "image/webp")]
    Webp,
    #[serde(rename = "image/gif")]
    Gif,
    #[serde(untagged)]
    Other(String),
}

/// A request to generate one or more images. Only `OpenAiClient` implements
/// `ImageProvider` today (see `lib::llm`) — Ollama's image-generation
/// endpoint is macOS-only and Anthropic doesn't offer one.
///
/// `size`/`quality`/`background`/`output_format` are passed through verbatim
/// rather than typed as an enum here: their allowed values are
/// OpenAI-specific and this crate should not bake one provider's vocabulary
/// into the shared wire types. The provider's own `wire` module validates
/// them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageRequest {
    pub prompt: String,
    /// Which model to call. Required — see `ChatOptions::model`'s doc for
    /// why there's no configured default to fall back to.
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    /// One of `"png"`, `"jpeg"`, `"webp"`. `None` uses the provider's own
    /// default (`png` for OpenAI's `gpt-image` models).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_format: Option<String>,
    #[serde(default = "default_image_count")]
    pub n: u32,
}

fn default_image_count() -> u32 {
    1
}

impl ImageRequest {
    pub fn new(model: impl Into<String>, prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            model: model.into(),
            size: None,
            quality: None,
            background: None,
            output_format: None,
            n: default_image_count(),
        }
    }
}

/// One generated image.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedImage {
    pub media_type: MediaType,
    pub data: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revised_prompt: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_data_serializes_as_a_string_not_a_byte_array() {
        let source = ImageSource {
            media_type: MediaType::Png,
            data: "aGVsbG8=".to_string(),
        };
        let json = serde_json::to_value(&source).unwrap();
        assert_eq!(json["data"], serde_json::json!("aGVsbG8="));
        assert!(
            json["data"].is_string(),
            "image data must serialize as a base64 string, not a byte array: {json}"
        );
    }

    #[test]
    fn media_type_round_trips_as_a_bare_mime_string() {
        let json = serde_json::to_value(MediaType::Jpeg).unwrap();
        assert_eq!(json, serde_json::json!("image/jpeg"));
        let back: MediaType = serde_json::from_value(json).unwrap();
        assert_eq!(back, MediaType::Jpeg);
    }

    #[test]
    fn unknown_media_types_preserve_the_original_string() {
        let parsed: MediaType = serde_json::from_str(r#""image/avif""#).unwrap();
        assert_eq!(parsed, MediaType::Other("image/avif".to_string()));
        assert_eq!(
            serde_json::to_value(&parsed).unwrap(),
            serde_json::json!("image/avif")
        );
    }

    #[test]
    fn image_request_omits_absent_optional_fields() {
        let req = ImageRequest::new("gpt-image-2", "a red cube");
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["model"], serde_json::json!("gpt-image-2"));
        assert!(json.get("size").is_none());
        assert!(json.get("quality").is_none());
        assert!(json.get("background").is_none());
        assert!(json.get("output_format").is_none());
    }
}
