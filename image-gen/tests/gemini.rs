#![cfg(feature = "gemini-tests")]

//! Exercises the Gemini image and LLM providers against the real API. Needs
//! `GEMINI_API_KEY` and makes billable requests. Run with
//! `cargo test --features gemini-tests`.

use image_gen::gemini::config::GeminiImageConfig;
use image_gen::gemini::image::{ImageRequest, generate};
use image_gen::gemini::{Client, GeminiConfig};
use image_gen::llm::gemini::GeminiProvider;
use image_gen::llm::{ChatRequest, Image, Message, Provider};

fn client() -> Client {
    Client::new(&GeminiConfig::default()).expect("set GEMINI_API_KEY to run the gemini tests")
}

fn model() -> String {
    std::env::var("IMAGE_GEN_GEMINI_MODEL").unwrap_or_else(|_| "gemini-2.5-flash".to_owned())
}

fn red_png() -> Vec<u8> {
    let mut out = Vec::new();
    image::RgbImage::from_pixel(16, 16, image::Rgb([220, 20, 20]))
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .unwrap();
    out
}

#[tokio::test]
async fn plain_chat_round_trip() {
    let request = ChatRequest {
        model: model(),
        messages: vec![Message::user("Reply with the single word: pong. Nothing else.")],
        tools: Vec::new(),
        options: Default::default(),
    };
    let response = GeminiProvider::new(client()).chat(&request).await.unwrap();
    assert!(!response.message.text().trim().is_empty());
}

#[tokio::test]
async fn vision_round_trip() {
    let image = Image::from_bytes(red_png()).unwrap();
    let request = ChatRequest {
        model: model(),
        messages: vec![Message::user_with_images(
            "What color is this image? Answer in one word.",
            vec![image],
        )],
        tools: Vec::new(),
        options: Default::default(),
    };
    let response = GeminiProvider::new(client()).chat(&request).await.unwrap();
    assert!(response.message.text().to_lowercase().contains("red"));
}

#[tokio::test]
async fn image_generation_returns_a_png() {
    let request = ImageRequest {
        prompt: "a plain red circle on a white background",
        ref_images: &[],
        seed: 1,
        width: Some(512),
        height: Some(512),
    };
    let bytes = generate(&client(), &GeminiImageConfig::default(), &request)
        .await
        .unwrap();
    assert_eq!(image::guess_format(&bytes).unwrap(), image::ImageFormat::Png);
}
