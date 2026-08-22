//! Shared helpers for the live-only integration tests in this directory.
//!
//! Every test file here is compiled by a plain `cargo test` — that's
//! intentional, so they never silently rot — but every test inside them
//! returns early unless `AI_HARNESS_LIVE=1` is set. See `README.md`'s
//! "Tests" section and `CLAUDE.md` for the unit-vs-live split this exists
//! to enforce.
//!
//! Each `live_*.rs` file is its own compilation unit and `mod`s this file in
//! separately, so any one binary only uses a subset of these helpers — the
//! rest report as dead code in *that* binary. Harmless; allowed wholesale
//! rather than picked apart per binary.
#![allow(dead_code)]

use std::time::Duration;

use lib::llm::config::{AnthropicConfig, OllamaConfig, OpenAiConfig};

/// A tiny (4x4, solid red) PNG, generated once and checked in, so the vision
/// test doesn't depend on any file outside the repo.
pub const PIXEL_PNG: &[u8] = include_bytes!("pixel.png");

pub fn pixel_png_base64() -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(PIXEL_PNG)
}

/// A generous but bounded timeout — live calls should fail fast rather than
/// hang a CI-less local run indefinitely.
pub const TIMEOUT: Duration = Duration::from_secs(120);

/// Whether live tests are opted into for this run. Live tests spend real
/// money (Anthropic, OpenAI) or need a running local service (Ollama) — a
/// plain `cargo test` must never trip them.
///
/// Loads `.env` (see `.env.example` at the repo root) first, so the API keys
/// these tests need can live there instead of the shell — a variable already
/// set in the environment still wins over the file.
pub fn live_enabled() -> bool {
    let _ = dotenvy::dotenv();
    std::env::var("AI_HARNESS_LIVE").as_deref() == Ok("1")
}

/// Call at the top of every live test, before doing anything that costs
/// money or needs a network. Prints why it's skipping and returns `true` if
/// the test should return immediately.
pub fn skip_unless_live(test_name: &str) -> bool {
    if !live_enabled() {
        eprintln!("skipping {test_name}: set AI_HARNESS_LIVE=1 to run live tests");
        true
    } else {
        false
    }
}

/// The Anthropic config to test against, honoring
/// `AI_HARNESS_LIVE_ANTHROPIC_MODEL` so a smoke run can point at something
/// cheaper (e.g. `claude-haiku-4-5`) without touching the default config.
pub fn anthropic_config() -> AnthropicConfig {
    let mut cfg = AnthropicConfig::default();
    if let Ok(model) = std::env::var("AI_HARNESS_LIVE_ANTHROPIC_MODEL") {
        cfg.model = model;
    }
    cfg
}

/// The Ollama config to test against, honoring `AI_HARNESS_LIVE_OLLAMA_MODEL`.
pub fn ollama_config() -> OllamaConfig {
    let mut cfg = OllamaConfig::default();
    if let Ok(model) = std::env::var("AI_HARNESS_LIVE_OLLAMA_MODEL") {
        cfg.model = model;
    }
    cfg
}

/// The OpenAI config to test against, honoring `AI_HARNESS_LIVE_OPENAI_MODEL`
/// and `AI_HARNESS_LIVE_OPENAI_IMAGE_MODEL` (e.g. to point at the cheaper
/// `gpt-image-1-mini` for a smoke run).
pub fn openai_config() -> OpenAiConfig {
    let mut cfg = OpenAiConfig::default();
    if let Ok(model) = std::env::var("AI_HARNESS_LIVE_OPENAI_MODEL") {
        cfg.model = model;
    }
    if let Ok(model) = std::env::var("AI_HARNESS_LIVE_OPENAI_IMAGE_MODEL") {
        cfg.image_model = model;
    }
    cfg
}
