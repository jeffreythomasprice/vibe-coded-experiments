use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde_json::{Value, json};

use crate::gemini::config::GeminiImageConfig;
use crate::gemini::client::RawResponse;
use crate::gemini::{Client, GeminiError};
use crate::llm::Image;

const SUPPORTED_ASPECT_RATIOS: [&str; 10] = [
    "1:1", "2:3", "3:2", "3:4", "4:3", "4:5", "5:4", "9:16", "16:9", "21:9",
];
const SUPPORTED_LIST: &str = "1:1, 2:3, 3:2, 3:4, 4:3, 4:5, 5:4, 9:16, 16:9, 21:9";

pub struct ImageRequest<'a> {
    pub prompt: &'a str,
    pub ref_images: &'a [Image],
    pub seed: i64,
    pub width: Option<i32>,
    pub height: Option<i32>,
}

/// Gemini takes an aspect ratio rather than pixel dimensions, so `-W`/`-H` are
/// accepted only when together they reduce to one of its supported ratios.
pub fn aspect_ratio(width: Option<i32>, height: Option<i32>) -> Result<Option<&'static str>, GeminiError> {
    let (width, height) = match (width, height) {
        (None, None) => return Ok(None),
        (Some(w), Some(h)) => (w, h),
        _ => return Err(GeminiError::IncompleteDimensions),
    };
    let divisor = gcd(width.unsigned_abs(), height.unsigned_abs()).max(1) as i32;
    let ratio = format!("{}:{}", width / divisor, height / divisor);
    SUPPORTED_ASPECT_RATIOS
        .iter()
        .find(|supported| **supported == ratio)
        .map(|supported| Some(*supported))
        .ok_or(GeminiError::UnsupportedAspectRatio {
            width,
            height,
            ratio,
            supported: SUPPORTED_LIST,
        })
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

pub fn request_body(
    request: &ImageRequest<'_>,
    config: &GeminiImageConfig,
    aspect_ratio: Option<&str>,
) -> Value {
    let mut parts = vec![json!({ "text": request.prompt })];
    parts.extend(request.ref_images.iter().map(|image| {
        json!({ "inlineData": { "mimeType": image.media_type.as_mime(), "data": image.to_base64() } })
    }));

    let mut image_config = serde_json::Map::new();
    if let Some(ratio) = aspect_ratio {
        image_config.insert("aspectRatio".to_owned(), json!(ratio));
    }
    if let Some(size) = &config.image_size {
        image_config.insert("imageSize".to_owned(), json!(size));
    }

    // The API's seed is an int32.
    let mut generation_config = json!({
        "responseModalities": ["IMAGE"],
        "seed": request.seed.rem_euclid(1 << 31),
    });
    if !image_config.is_empty() {
        generation_config["imageConfig"] = Value::Object(image_config);
    }

    json!({
        "contents": [{ "role": "user", "parts": parts }],
        "generationConfig": generation_config,
    })
}

/// Returns PNG bytes, re-encoding when the model answered in another format.
pub async fn generate(
    client: &Client,
    config: &GeminiImageConfig,
    request: &ImageRequest<'_>,
) -> Result<Vec<u8>, GeminiError> {
    let ratio = aspect_ratio(request.width, request.height)?;
    let body = request_body(request, config, ratio);
    png_from_response(client.generate_content(&config.model, &body).await?)
}

pub fn png_from_response(response: RawResponse) -> Result<Vec<u8>, GeminiError> {
    let candidate = response.first_candidate()?;
    let inline = candidate
        .parts()
        .iter()
        .filter(|part| !part.thought)
        .find_map(|part| part.inline_data.as_ref());
    let Some(inline) = inline else {
        return Err(GeminiError::NoImage {
            finish_reason: candidate.finish_reason.clone().unwrap_or_else(|| "unknown".to_owned()),
            finish_message: candidate
                .finish_message
                .as_deref()
                .map(|message| format!(": {message}"))
                .unwrap_or_default(),
            text: candidate.text(),
        });
    };

    let bytes = BASE64.decode(&inline.data).map_err(GeminiError::ImageBase64)?;
    if inline.mime_type == "image/png" {
        return Ok(bytes);
    }
    let decoded = ::image::load_from_memory(&bytes).map_err(GeminiError::ImageReencode)?;
    let mut png = Vec::new();
    decoded
        .write_to(&mut std::io::Cursor::new(&mut png), ::image::ImageFormat::Png)
        .map_err(GeminiError::ImageReencode)?;
    Ok(png)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gemini::GeminiConfig;
    use crate::llm::MediaType;
    use wiremock::matchers::{body_partial_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn client_for(server: &MockServer) -> Client {
        Client::new(&GeminiConfig {
            api_key: Some("k".to_owned()),
            base_url: server.uri(),
            timeout_secs: 5,
            ..Default::default()
        })
        .unwrap()
    }

    fn request<'a>(prompt: &'a str, ref_images: &'a [Image]) -> ImageRequest<'a> {
        ImageRequest {
            prompt,
            ref_images,
            seed: 7,
            width: None,
            height: None,
        }
    }

    fn encoded(format: ::image::ImageFormat) -> Vec<u8> {
        let mut out = Vec::new();
        ::image::RgbImage::from_pixel(2, 2, ::image::Rgb([10, 20, 30]))
            .write_to(&mut std::io::Cursor::new(&mut out), format)
            .unwrap();
        out
    }

    fn image_reply(mime: &str, bytes: &[u8]) -> Value {
        json!({"candidates": [{
            "content": {"parts": [{"inlineData": {"mimeType": mime, "data": BASE64.encode(bytes)}}]},
            "finishReason": "STOP"
        }]})
    }

    #[test]
    fn aspect_ratio_reduces_dimensions() {
        assert_eq!(aspect_ratio(Some(1024), Some(768)).unwrap(), Some("4:3"));
        assert_eq!(aspect_ratio(Some(512), Some(512)).unwrap(), Some("1:1"));
        assert_eq!(aspect_ratio(Some(1920), Some(1080)).unwrap(), Some("16:9"));
        assert_eq!(aspect_ratio(None, None).unwrap(), None);
    }

    #[test]
    fn unsupported_ratio_names_the_reduced_ratio() {
        match aspect_ratio(Some(1000), Some(700)) {
            Err(GeminiError::UnsupportedAspectRatio { ratio, .. }) => assert_eq!(ratio, "10:7"),
            other => panic!("expected UnsupportedAspectRatio, got {other:?}"),
        }
    }

    #[test]
    fn a_single_dimension_is_rejected() {
        assert!(matches!(
            aspect_ratio(Some(512), None),
            Err(GeminiError::IncompleteDimensions)
        ));
    }

    #[test]
    fn body_has_prompt_ref_images_and_config() {
        let png = Image {
            media_type: MediaType::Png,
            data: vec![1, 2, 3],
        };
        let refs = [png];
        let config = GeminiImageConfig {
            image_size: Some("2K".to_owned()),
            ..Default::default()
        };
        let body = request_body(&request("a cat", &refs), &config, Some("4:3"));

        assert_eq!(body["contents"][0]["role"], "user");
        assert_eq!(body["contents"][0]["parts"][0]["text"], "a cat");
        assert_eq!(body["contents"][0]["parts"][1]["inlineData"]["mimeType"], "image/png");
        assert_eq!(body["generationConfig"]["responseModalities"][0], "IMAGE");
        assert_eq!(body["generationConfig"]["seed"], 7);
        assert_eq!(body["generationConfig"]["imageConfig"]["aspectRatio"], "4:3");
        assert_eq!(body["generationConfig"]["imageConfig"]["imageSize"], "2K");
    }

    #[test]
    fn body_omits_image_config_when_nothing_is_set() {
        let body = request_body(&request("a cat", &[]), &GeminiImageConfig::default(), None);
        assert!(body["generationConfig"].get("imageConfig").is_none());
    }

    #[test]
    fn oversized_seed_is_folded_into_int32_range() {
        let mut req = request("a cat", &[]);
        req.seed = (1 << 40) + 5;
        let body = request_body(&req, &GeminiImageConfig::default(), None);
        assert_eq!(body["generationConfig"]["seed"], 5);
    }

    #[tokio::test]
    async fn returns_png_bytes_unchanged() {
        let server = MockServer::start().await;
        let png = encoded(::image::ImageFormat::Png);
        Mock::given(method("POST"))
            .and(path("/models/gemini-2.5-flash-image:generateContent"))
            .and(body_partial_json(json!({"generationConfig": {"responseModalities": ["IMAGE"]}})))
            .respond_with(ResponseTemplate::new(200).set_body_json(image_reply("image/png", &png)))
            .mount(&server)
            .await;

        let out = generate(&client_for(&server), &GeminiImageConfig::default(), &request("p", &[]))
            .await
            .unwrap();
        assert_eq!(out, png);
    }

    #[tokio::test]
    async fn jpeg_reply_is_reencoded_to_png() {
        let server = MockServer::start().await;
        let jpeg = encoded(::image::ImageFormat::Jpeg);
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(image_reply("image/jpeg", &jpeg)))
            .mount(&server)
            .await;

        let out = generate(&client_for(&server), &GeminiImageConfig::default(), &request("p", &[]))
            .await
            .unwrap();
        assert_eq!(::image::guess_format(&out).unwrap(), ::image::ImageFormat::Png);
    }

    #[tokio::test]
    async fn text_only_reply_is_no_image() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"candidates": [{
                "content": {"parts": [{"text": "I can't draw that"}]},
                "finishReason": "STOP"
            }]})))
            .mount(&server)
            .await;

        let err = generate(&client_for(&server), &GeminiImageConfig::default(), &request("p", &[]))
            .await
            .unwrap_err();
        match err {
            GeminiError::NoImage { finish_reason, text, .. } => {
                assert_eq!(finish_reason, "STOP");
                assert_eq!(text, "I can't draw that");
            }
            other => panic!("expected NoImage, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn blocked_prompt_is_blocked() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"promptFeedback": {"blockReason": "SAFETY"}})),
            )
            .mount(&server)
            .await;

        let err = generate(&client_for(&server), &GeminiImageConfig::default(), &request("p", &[]))
            .await
            .unwrap_err();
        assert!(matches!(err, GeminiError::Blocked { .. }));
    }
}
