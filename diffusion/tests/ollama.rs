#![cfg(feature = "ollama-tests")]

//! Exercises `llm::ollama::OllamaProvider` against a real Ollama server, expected at
//! `localhost:11434`. Run with `cargo test --features ollama-tests`; omitted from a
//! plain `cargo test`, so this suite never runs unless explicitly opted into.

use std::sync::Arc;
use std::time::Duration;

use diffusion::llm::{
    Agent, ChatOptions, ChatRequest, FunctionTool, Message, Provider, StopReason, ToolOutput,
    ToolRegistry,
};
use diffusion::llm::ollama::OllamaProvider;
use schemars::JsonSchema;
use serde::Deserialize;

fn model() -> String {
    std::env::var("DIFFUSION_OLLAMA_MODEL").unwrap_or_else(|_| "qwen3:4b".to_owned())
}

fn provider() -> OllamaProvider {
    OllamaProvider::new("http://localhost:11434", Duration::from_secs(120), Duration::from_secs(600))
}

async fn require_ollama_running() {
    let response = reqwest::get("http://localhost:11434/api/tags").await;
    assert!(
        response.is_ok_and(|r| r.status().is_success()),
        "expected an Ollama server at localhost:11434 (start it with `ollama serve`)"
    );
}

#[tokio::test]
async fn plain_chat_round_trip() {
    require_ollama_running().await;

    let request = ChatRequest {
        model: model(),
        messages: vec![Message::user(
            "Reply with the single word: pong. Nothing else.",
        )],
        tools: Vec::new(),
        options: Default::default(),
    };
    let response = provider().chat(&request).await.unwrap();
    assert!(!response.message.text().trim().is_empty());
}

#[tokio::test]
async fn vision_round_trip() {
    require_ollama_running().await;

    let png = one_pixel_png();
    let image = diffusion::llm::Image::from_bytes(png).unwrap();
    let request = ChatRequest {
        model: model(),
        messages: vec![Message::user_with_images(
            "What color is this image? Answer in one word.",
            vec![image],
        )],
        tools: Vec::new(),
        options: Default::default(),
    };
    let response = provider().chat(&request).await.unwrap();
    assert!(!response.message.text().trim().is_empty());
}

/// Exercises the wire path VQAScore depends on: `think: false` so a reasoning
/// model doesn't burn the single generated token on a thinking token, and
/// `logprobs`/`top_logprobs` as top-level request keys rather than nested under
/// `options`.
#[tokio::test]
async fn logprobs_round_trip() {
    require_ollama_running().await;

    let request = ChatRequest {
        model: model(),
        messages: vec![Message::user("Is water wet? Answer yes or no.")],
        tools: Vec::new(),
        options: ChatOptions {
            max_tokens: Some(1),
            think: Some(false),
            temperature: Some(0.0),
            logprobs: Some(5),
            ..Default::default()
        },
    };
    let response = provider().chat(&request).await.unwrap();

    assert_eq!(response.stop_reason, StopReason::Length);
    assert!(!response.logprobs.is_empty(), "expected at least one token of logprobs");
    assert!(
        !response.logprobs[0].top.is_empty(),
        "expected top-token alternatives, got {:?}",
        response.logprobs
    );
}

#[derive(Debug, Deserialize, JsonSchema)]
struct AddArgs {
    a: i64,
    b: i64,
}

#[tokio::test]
async fn agent_loop_runs_a_registered_tool() {
    require_ollama_running().await;

    let mut tools = ToolRegistry::new();
    tools
        .register(FunctionTool::sync::<AddArgs, _>(
            "add",
            "Adds two integers and returns the sum",
            |args| Ok(ToolOutput::text((args.a + args.b).to_string())),
        ))
        .unwrap();

    let agent = Agent::new(Arc::new(provider()), model())
        .with_tools(tools)
        .with_max_turns(5);

    let outcome = agent
        .run(vec![Message::user(
            "Use the add tool to compute 17 + 25, then tell me the result.",
        )])
        .await
        .unwrap();

    assert!(
        outcome.messages.iter().any(|m| m.tool_name.as_deref() == Some("add")),
        "expected the agent to have called the add tool; transcript: {:?}",
        outcome.messages
    );
}

fn one_pixel_png() -> Vec<u8> {
    // A minimal valid 1x1 red PNG.
    vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90,
        0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63, 0xF8,
        0xCF, 0xC0, 0x00, 0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xDD, 0x8D, 0xB0, 0x00, 0x00, 0x00,
        0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ]
}
