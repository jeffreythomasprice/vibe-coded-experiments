//! OpenAI Images API (`POST {base_url}/v1/images/generations`) request/
//! response shapes and pure conversions to/from `shared::llm` types.
//!
//! This is the one capability none of the other two providers can offer on
//! this project's target platform: Ollama's image-generation endpoint is
//! macOS-only (verified against a local Linux install returning
//! `"image generation models are not currently supported"`) and Anthropic
//! doesn't have an image-generation endpoint at all.

use serde::de::Error as _;
use serde::Deserialize;

use shared::llm::{GeneratedImage, ImageRequest, MediaType};

use crate::llm::config::OpenAiConfig;
use crate::llm::error::LlmError;

use super::wire::PROVIDER;

/// Build the JSON body for `POST {base_url}/v1/images/generations`.
pub fn build_request(request: &ImageRequest, cfg: &OpenAiConfig) -> serde_json::Value {
    let mut body = serde_json::json!({
        "model": request.model.clone().unwrap_or_else(|| cfg.image_model.clone()),
        "prompt": request.prompt,
        "n": request.n,
    });
    if let Some(size) = &request.size {
        body["size"] = serde_json::json!(size);
    }
    if let Some(quality) = &request.quality {
        body["quality"] = serde_json::json!(quality);
    }
    if let Some(background) = &request.background {
        body["background"] = serde_json::json!(background);
    }
    if let Some(output_format) = &request.output_format {
        body["output_format"] = serde_json::json!(output_format);
    }
    body
}

#[derive(Debug, Deserialize)]
struct WireImagesResponse {
    #[serde(default)]
    data: Vec<WireImageData>,
}

#[derive(Debug, Deserialize)]
struct WireImageData {
    #[serde(default)]
    b64_json: Option<String>,
    #[serde(default)]
    revised_prompt: Option<String>,
}

/// `output_format` is what was *requested* (`None` defaults to `"png"`,
/// matching OpenAI's own default for `gpt-image` models) — the response body
/// doesn't echo it back per image, so there's nothing to read it from.
pub fn parse_response(
    body: &str,
    output_format: Option<&str>,
) -> Result<Vec<GeneratedImage>, LlmError> {
    let wire: WireImagesResponse = serde_json::from_str(body).map_err(|source| LlmError::Decode {
        provider: PROVIDER,
        context: "images.generations response".to_string(),
        source,
    })?;
    let media_type = output_format_to_media_type(output_format);
    wire.data
        .into_iter()
        .map(|item| {
            let data = item.b64_json.ok_or_else(|| LlmError::Decode {
                provider: PROVIDER,
                context: "images.generations response".to_string(),
                // gpt-image models always return base64 (never a URL), so a
                // missing `b64_json` means the response shape didn't match
                // what this crate expects. Synthesizing a decode error here
                // avoids adding a whole new `LlmError` variant for this one
                // edge case.
                source: serde_json::Error::custom("response item is missing b64_json"),
            })?;
            Ok(GeneratedImage {
                media_type: media_type.clone(),
                data,
                revised_prompt: item.revised_prompt,
            })
        })
        .collect()
}

fn output_format_to_media_type(output_format: Option<&str>) -> MediaType {
    match output_format {
        Some("jpeg") | Some("jpg") => MediaType::Jpeg,
        Some("webp") => MediaType::Webp,
        Some("png") | None => MediaType::Png,
        Some(other) => MediaType::Other(format!("image/{other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RESPONSE_IMAGES: &str = include_str!("fixtures/response_images.json");

    #[test]
    fn builds_a_minimal_images_request() {
        let req = ImageRequest {
            prompt: "a red cube on a white table".to_string(),
            model: None,
            size: Some("1024x1024".to_string()),
            quality: Some("high".to_string()),
            background: None,
            output_format: None,
            n: 1,
        };
        let body = build_request(&req, &OpenAiConfig::default());
        assert_eq!(body["model"], serde_json::json!("gpt-image-2"));
        assert_eq!(
            body["prompt"],
            serde_json::json!("a red cube on a white table")
        );
        assert_eq!(body["size"], serde_json::json!("1024x1024"));
        assert_eq!(body["quality"], serde_json::json!("high"));
        assert!(body.get("background").is_none());
        assert!(body.get("output_format").is_none());
    }

    #[test]
    fn parses_an_images_generations_response_into_generated_images() {
        let images = parse_response(RESPONSE_IMAGES, Some("png")).unwrap();
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].media_type, MediaType::Png);
        assert_eq!(images[0].data, "aGVsbG8gd29ybGQ=");
        assert_eq!(
            images[0].revised_prompt.as_deref(),
            Some("A red cube on a white table, studio lighting.")
        );
    }

    #[test]
    fn defaults_to_png_when_no_output_format_was_requested() {
        let images = parse_response(RESPONSE_IMAGES, None).unwrap();
        assert_eq!(images[0].media_type, MediaType::Png);
    }

    #[test]
    fn jpeg_and_webp_map_to_the_right_media_type() {
        assert_eq!(output_format_to_media_type(Some("jpeg")), MediaType::Jpeg);
        assert_eq!(output_format_to_media_type(Some("webp")), MediaType::Webp);
    }
}
