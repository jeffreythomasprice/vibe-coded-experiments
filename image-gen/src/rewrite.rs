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

/// Compresses `prompt` with `model`. Falls back to the original prompt (with a
/// warning) rather than erroring when the model replies with nothing usable —
/// a failed rewrite should degrade to "no rewrite", not abort generation.
pub fn rewrite(llm: &Llm, model: &str, prompt: &str) -> Result<String, LlmError> {
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
    let response = llm.chat(&request)?;
    let rewritten = response.message.text().trim().to_owned();
    if rewritten.is_empty() {
        tracing::warn!(model, "rewrite returned no text; using the original prompt");
        return Ok(prompt.to_owned());
    }
    Ok(rewritten)
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

    #[test]
    fn rewrite_returns_trimmed_model_text() {
        let provider = Arc::new(ScriptedProvider::new(vec![reply("  a tidy prompt  ")]));
        let llm = Llm::test_with_provider(provider);
        let result = rewrite(&llm, "test-model", "a very long messy prompt").unwrap();
        assert_eq!(result, "a tidy prompt");
    }

    #[test]
    fn empty_rewrite_falls_back_to_the_original_prompt() {
        let provider = Arc::new(ScriptedProvider::new(vec![reply("   ")]));
        let llm = Llm::test_with_provider(provider);
        let result = rewrite(&llm, "test-model", "the original").unwrap();
        assert_eq!(result, "the original");
    }

    #[test]
    fn rewrite_sends_think_false_and_low_temperature() {
        let provider = Arc::new(ScriptedProvider::new(vec![reply("ok")]));
        let llm = Llm::test_with_provider(provider.clone());
        rewrite(&llm, "test-model", "prompt").unwrap();
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
