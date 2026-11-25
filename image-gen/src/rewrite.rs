use crate::cli::RewriteMode;
use crate::llm::{ChatOptions, ChatRequest, Llm, LlmError, Message};

pub const DEFAULT_THRESHOLD: usize = 300;

const SYSTEM_PROMPT: &str = "You turn a long, possibly messy image description into a \
single short prompt suitable for a text-to-image diffusion model. Keep every concrete \
visual requirement: subjects, counts, colors, poses, composition, setting, style. Drop \
narrative framing, meta-commentary, and anything addressed to a reader rather than \
describing the image. Reply with only the rewritten prompt, nothing else.";

pub fn should_rewrite(mode: RewriteMode, prompt: &str, threshold: usize) -> bool {
    match mode {
        RewriteMode::Always => true,
        RewriteMode::Never => false,
        RewriteMode::Auto => prompt.chars().count() > threshold,
    }
}

const THINK_CLOSE: &str = "</think>";
const THINK_OPEN: &str = "<think>";

/// Reasoning models can ignore `think: false` and return their chain of thought
/// as content, sometimes with only the closing tag. Keep what follows the last
/// closing tag, after dropping any complete `<think>…</think>` blocks.
fn strip_reasoning(text: &str) -> &str {
    match text.rfind(THINK_CLOSE) {
        Some(end) => &text[end + THINK_CLOSE.len()..],
        None => match text.find(THINK_OPEN) {
            Some(_) => "",
            None => text,
        },
    }
}

/// Compresses `prompt` with `model`. Falls back to the original prompt (with a
/// warning) rather than erroring when the model replies with nothing usable or
/// with something longer than the prompt it was meant to shorten — a failed
/// rewrite should degrade to "no rewrite", not abort generation.
pub async fn rewrite(llm: &Llm, model: &str, prompt: &str) -> Result<String, LlmError> {
    let request = ChatRequest {
        model: model.to_owned(),
        messages: vec![Message::system(SYSTEM_PROMPT), Message::user(prompt)],
        tools: Vec::new(),
        options: ChatOptions {
            think: Some(false),
            temperature: Some(0.2),
            ..Default::default()
        },
    };
    let response = llm.chat(&request).await?;
    let raw = response.message.text();
    let rewritten = strip_reasoning(&raw).trim();
    if rewritten.is_empty() {
        tracing::warn!(model, "rewrite returned no text; using the original prompt");
        return Ok(prompt.to_owned());
    }
    if rewritten.chars().count() > prompt.chars().count() {
        tracing::warn!(
            model,
            original_chars = prompt.chars().count(),
            rewritten_chars = rewritten.chars().count(),
            "rewrite is longer than the original; using the original prompt"
        );
        return Ok(prompt.to_owned());
    }
    Ok(rewritten.to_owned())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::llm::mock::ScriptedProvider;
    use crate::llm::{ChatResponse, Message, StopReason, Usage};

    fn reply(text: &str) -> ChatResponse {
        ChatResponse {
            message: Message::assistant(text),
            stop_reason: StopReason::Stop,
            usage: Usage::default(),
            logprobs: Vec::new(),
        }
    }

    #[tokio::test]
    async fn rewrite_returns_trimmed_model_text() {
        let provider = Arc::new(ScriptedProvider::new(vec![reply("  a tidy prompt  ")]));
        let llm = Llm::test_with_provider(provider);
        let result = rewrite(&llm, "test-model", "a very long messy prompt")
            .await
            .unwrap();
        assert_eq!(result, "a tidy prompt");
    }

    #[tokio::test]
    async fn empty_rewrite_falls_back_to_the_original_prompt() {
        let provider = Arc::new(ScriptedProvider::new(vec![reply("   ")]));
        let llm = Llm::test_with_provider(provider);
        let result = rewrite(&llm, "test-model", "the original").await.unwrap();
        assert_eq!(result, "the original");
    }

    #[tokio::test]
    async fn reasoning_before_a_lone_closing_tag_is_dropped() {
        let provider = Arc::new(ScriptedProvider::new(vec![reply(
            "Let me think about this.\nMaybe a cat.\n</think>\n\nA cat on a mat",
        )]));
        let llm = Llm::test_with_provider(provider);
        let original = "a very long messy prompt about a cat sitting on a mat in the sun";
        let result = rewrite(&llm, "test-model", original).await.unwrap();
        assert_eq!(result, "A cat on a mat");
    }

    #[tokio::test]
    async fn complete_think_block_is_dropped() {
        let provider = Arc::new(ScriptedProvider::new(vec![reply(
            "<think>hmm</think>A cat on a mat",
        )]));
        let llm = Llm::test_with_provider(provider);
        let original = "a very long messy prompt about a cat sitting on a mat in the sun";
        let result = rewrite(&llm, "test-model", original).await.unwrap();
        assert_eq!(result, "A cat on a mat");
    }

    #[tokio::test]
    async fn unterminated_think_block_falls_back_to_the_original_prompt() {
        let provider = Arc::new(ScriptedProvider::new(vec![reply("<think>still thinking")]));
        let llm = Llm::test_with_provider(provider);
        let result = rewrite(&llm, "test-model", "the original").await.unwrap();
        assert_eq!(result, "the original");
    }

    #[tokio::test]
    async fn rewrite_longer_than_the_original_falls_back_to_the_original_prompt() {
        let provider = Arc::new(ScriptedProvider::new(vec![reply(&"word ".repeat(100))]));
        let llm = Llm::test_with_provider(provider);
        let result = rewrite(&llm, "test-model", "the original").await.unwrap();
        assert_eq!(result, "the original");
    }

    #[tokio::test]
    async fn rewrite_sends_think_false_and_low_temperature() {
        let provider = Arc::new(ScriptedProvider::new(vec![reply("ok")]));
        let llm = Llm::test_with_provider(provider.clone());
        rewrite(&llm, "test-model", "prompt").await.unwrap();
        let sent = &provider.requests()[0];
        assert_eq!(sent.options.think, Some(false));
        assert_eq!(sent.options.temperature, Some(0.2));
    }

    #[test]
    fn never_mode_never_rewrites() {
        assert!(!should_rewrite(RewriteMode::Never, &"x".repeat(10_000), 10));
    }

    #[test]
    fn always_mode_always_rewrites() {
        assert!(should_rewrite(RewriteMode::Always, "short", 10_000));
    }

    #[test]
    fn auto_mode_triggers_above_threshold_only() {
        assert!(!should_rewrite(RewriteMode::Auto, "12345", 5));
        assert!(should_rewrite(RewriteMode::Auto, "123456", 5));
    }
}
