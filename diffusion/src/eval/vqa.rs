use std::path::Path;

use serde::Serialize;

use crate::eval::EvalError;
use crate::llm::{ChatOptions, ChatRequest, Llm, Message, TokenChoice};

/// Ollama caps `top_logprobs` at 20 (a higher value is a 400); there is no
/// benefit to asking for fewer since the whole top-20 is scanned for yes/no
/// tokens anyway, so this isn't exposed as a user-facing knob.
const TOP_LOGPROBS: u8 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct VqaResult {
    pub score: f32,
}

/// *Evaluating Text-to-Visual Generation with Image-to-Text Generation*
/// (arXiv 2404.01291): ask a VLM a yes/no question about the image and read the
/// probability of "yes" straight off the logprobs of a single generated token,
/// rather than generating and parsing any text.
pub fn score(
    llm: &Llm,
    model: &str,
    prompt: &str,
    path: &Path,
    max_px: u32,
) -> Result<VqaResult, EvalError> {
    let image = crate::eval::load_scaled(path, max_px)?;
    let question = format!(
        "Does this figure show \"{}\"? Please answer yes or no.",
        normalize(prompt)
    );
    let request = ChatRequest {
        model: model.to_owned(),
        messages: vec![Message::user_with_images(question, vec![image])],
        tools: Vec::new(),
        options: ChatOptions {
            max_tokens: Some(1),
            temperature: Some(0.0),
            think: Some(false),
            logprobs: Some(TOP_LOGPROBS),
            ..Default::default()
        },
    };
    // A 1-token request stops on `StopReason::Length`, not `Stop` — deliberately
    // not checked here; only the logprobs matter.
    let response = llm.chat(&request)?;
    score_from_logprobs(&response.logprobs)
}

/// Collapses a prompt to one line and drops embedded quotes so it can be
/// interpolated into `Does this figure show "{prompt}"?` without breaking the
/// question — relevant since long prompts (exactly what `--rewrite` exists for)
/// are the ones likely to contain a quote or a newline.
fn normalize(prompt: &str) -> String {
    prompt
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace('"', "'")
}

/// Sums `exp(logprob)` over every top-token whose trimmed, lowercased text is
/// "yes" or "no" — verified live to be necessary, since a model's top tokens
/// include case/whitespace variants (`Yes`, `yes`, `YES`, ` yes`) that must all
/// count toward the same side rather than only the single most likely spelling.
fn score_from_logprobs(logprobs: &[TokenChoice]) -> Result<VqaResult, EvalError> {
    let candidates = logprobs
        .first()
        .map(|choice| choice.top.as_slice())
        .unwrap_or(&[]);

    let mut p_yes = 0.0_f64;
    let mut p_no = 0.0_f64;
    let mut seen = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        seen.push(candidate.token.clone());
        match candidate.token.trim().to_lowercase().as_str() {
            "yes" => p_yes += f64::from(candidate.logprob).exp(),
            "no" => p_no += f64::from(candidate.logprob).exp(),
            _ => {}
        }
    }

    if p_yes == 0.0 && p_no == 0.0 {
        return Err(EvalError::NoAnswerToken {
            tokens: seen.join(", "),
        });
    }
    Ok(VqaResult {
        score: (p_yes / (p_yes + p_no)) as f32,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::llm::mock::ScriptedProvider;
    use crate::llm::{ChatResponse, StopReason, TokenLogprob, Usage};

    fn top(pairs: &[(&str, f32)]) -> TokenChoice {
        TokenChoice {
            token: pairs[0].0.to_owned(),
            logprob: pairs[0].1,
            top: pairs
                .iter()
                .map(|(token, logprob)| TokenLogprob {
                    token: (*token).to_owned(),
                    logprob: *logprob,
                })
                .collect(),
        }
    }

    #[test]
    fn clear_yes_scores_near_one() {
        let logprobs = vec![top(&[("Yes", -0.08), ("No", -8.0)])];
        let result = score_from_logprobs(&logprobs).unwrap();
        assert!(result.score > 0.99, "score was {}", result.score);
    }

    #[test]
    fn clear_no_scores_near_zero() {
        let logprobs = vec![top(&[("No", -0.08), ("Yes", -8.0)])];
        let result = score_from_logprobs(&logprobs).unwrap();
        assert!(result.score < 0.01, "score was {}", result.score);
    }

    #[test]
    fn case_and_whitespace_variants_are_summed_on_the_same_side() {
        let logprobs = vec![top(&[
            ("Yes", -1.0),
            ("yes", -1.5),
            ("YES", -2.0),
            (" yes", -3.0),
        ])];
        let result = score_from_logprobs(&logprobs).unwrap();
        assert!(result.score > 0.99, "score was {}", result.score);
    }

    #[test]
    fn neither_yes_nor_no_present_is_an_error() {
        let logprobs = vec![top(&[("The", -0.0), ("Thinking", -12.9)])];
        let err = score_from_logprobs(&logprobs).unwrap_err();
        match err {
            EvalError::NoAnswerToken { tokens } => {
                assert!(tokens.contains("The"));
            }
            other => panic!("expected NoAnswerToken, got {other:?}"),
        }
    }

    #[test]
    fn empty_logprobs_is_an_error_not_a_panic() {
        let err = score_from_logprobs(&[]).unwrap_err();
        assert!(matches!(err, EvalError::NoAnswerToken { .. }));
    }

    #[test]
    fn scoring_ignores_stop_reason_length_from_a_one_token_request() {
        let provider = Arc::new(ScriptedProvider::new(vec![ChatResponse {
            message: Message::assistant("Yes"),
            stop_reason: StopReason::Length,
            usage: Usage::default(),
            logprobs: vec![top(&[("Yes", -0.08), ("No", -8.0)])],
        }]));
        let llm = Llm::test_with_provider(provider);

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("image.png");
        image::RgbImage::from_pixel(4, 4, image::Rgb([200, 30, 30]))
            .save(&path)
            .unwrap();

        let result = score(&llm, "test-model", "a red square", &path, 512).unwrap();
        assert!(result.score > 0.99);
    }

    #[test]
    fn scoring_sends_think_false_and_one_max_token() {
        let provider = Arc::new(ScriptedProvider::new(vec![ChatResponse {
            message: Message::assistant("Yes"),
            stop_reason: StopReason::Length,
            usage: Usage::default(),
            logprobs: vec![top(&[("Yes", -0.08), ("No", -8.0)])],
        }]));
        let llm = Llm::test_with_provider(provider.clone());

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("image.png");
        image::RgbImage::from_pixel(4, 4, image::Rgb([200, 30, 30]))
            .save(&path)
            .unwrap();

        score(&llm, "test-model", "a red square", &path, 512).unwrap();

        let sent = &provider.requests()[0];
        assert_eq!(sent.options.think, Some(false));
        assert_eq!(sent.options.max_tokens, Some(1));
        assert_eq!(sent.options.logprobs, Some(TOP_LOGPROBS));
    }

    #[test]
    fn quotes_and_newlines_in_the_prompt_are_normalized() {
        let normalized = normalize("a \"wizard\"\nwith a hat");
        assert_eq!(normalized, "a 'wizard' with a hat");
    }
}
