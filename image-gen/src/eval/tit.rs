use std::path::Path;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::eval::EvalError;
use crate::llm::{ChatOptions, ChatRequest, Llm, Message};

const CAPTION_INSTRUCTION: &str = "Describe this image in 250 to 350 words. Describe only \
what is visible: subjects, setting, colors, composition, and mood. Do not guess at anything \
outside the frame, and do not mention that this is a generated image.";

const JUDGE_SYSTEM: &str = "You are checking an image caption against the prompt that was \
used to generate the image. Break the prompt into a list of atomic, independently checkable \
visual claims (subjects, counts, colors, poses, setting, style, ...), then judge each one \
against the caption as supported, contradicted, or absent (the caption neither confirms nor \
denies it).";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Supported,
    Contradicted,
    Absent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Claim {
    pub claim: String,
    pub verdict: Verdict,
}

#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
struct Judgment {
    claims: Vec<Claim>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TitResult {
    pub score: f32,
    pub claims: Vec<Claim>,
}

/// *TIT-Score* (arXiv 2510.02987): caption the image without showing it the
/// prompt or asking it to score anything, then judge the prompt against that
/// caption. That decoupling — the captioner never sees what answer is wanted —
/// is what keeps this metric from yes-biasing the way a single combined
/// caption-and-score call would.
pub async fn score(
    llm: &Llm,
    caption_model: &str,
    judge_model: &str,
    prompt: &str,
    path: &Path,
    max_px: u32,
) -> Result<TitResult, EvalError> {
    let caption_text = caption(llm, caption_model, path, max_px).await?;
    tracing::debug!(caption = %caption_text, "captioned image for TIT-Score");
    judge(llm, judge_model, prompt, &caption_text).await
}

async fn caption(llm: &Llm, model: &str, path: &Path, max_px: u32) -> Result<String, EvalError> {
    let image = crate::eval::load_scaled(path, max_px)?;
    let request = ChatRequest {
        model: model.to_owned(),
        messages: vec![Message::user_with_images(CAPTION_INSTRUCTION, vec![image])],
        tools: Vec::new(),
        options: ChatOptions {
            think: Some(false),
            temperature: Some(0.2),
            max_tokens: Some(600),
            ..Default::default()
        },
    };
    let response = llm.chat(&request).await?;
    Ok(response.message.text().trim().to_owned())
}

async fn judge(llm: &Llm, model: &str, prompt: &str, caption: &str) -> Result<TitResult, EvalError> {
    let user = format!("Prompt:\n{prompt}\n\nCaption:\n{caption}");
    let request = ChatRequest {
        model: model.to_owned(),
        messages: vec![Message::system(JUDGE_SYSTEM), Message::user(user)],
        tools: Vec::new(),
        options: ChatOptions {
            think: Some(false),
            temperature: Some(0.0),
            format: Some(crate::llm::tool::argument_schema::<Judgment>()),
            ..Default::default()
        },
    };
    let response = llm.chat(&request).await?;
    let judgment: Judgment = serde_json::from_str(&response.message.text())
        .map_err(|source| EvalError::Decode { source })?;
    claims_to_result(judgment.claims)
}

fn claims_to_result(claims: Vec<Claim>) -> Result<TitResult, EvalError> {
    if claims.is_empty() {
        return Err(EvalError::NoClaims);
    }
    let supported = claims
        .iter()
        .filter(|c| c.verdict == Verdict::Supported)
        .count();
    let score = supported as f32 / claims.len() as f32;
    Ok(TitResult { score, claims })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::llm::mock::ScriptedProvider;
    use crate::llm::{ChatResponse, StopReason, Usage};
    use serde_json::json;

    fn reply(text: impl Into<String>) -> ChatResponse {
        ChatResponse {
            message: Message::assistant(text),
            stop_reason: StopReason::Stop,
            usage: Usage::default(),
            logprobs: Vec::new(),
        }
    }

    #[test]
    fn score_is_supported_fraction() {
        let claims = vec![
            Claim {
                claim: "a red bicycle".to_owned(),
                verdict: Verdict::Supported,
            },
            Claim {
                claim: "on a beach".to_owned(),
                verdict: Verdict::Contradicted,
            },
            Claim {
                claim: "at sunset".to_owned(),
                verdict: Verdict::Absent,
            },
            Claim {
                claim: "with a basket".to_owned(),
                verdict: Verdict::Supported,
            },
        ];
        let result = claims_to_result(claims).unwrap();
        assert_eq!(result.score, 0.5);
    }

    #[test]
    fn zero_claims_is_an_error() {
        let err = claims_to_result(Vec::new()).unwrap_err();
        assert!(matches!(err, EvalError::NoClaims));
    }

    #[tokio::test]
    async fn judge_parses_structured_json_reply() {
        let body = json!({
            "claims": [
                {"claim": "a red bicycle", "verdict": "supported"},
                {"claim": "a blue car", "verdict": "contradicted"}
            ]
        });
        let provider = Arc::new(ScriptedProvider::new(vec![reply(body.to_string())]));
        let llm = Llm::test_with_provider(provider);

        let result = judge(
            &llm,
            "test-model",
            "a red bicycle",
            "a blue car sits on a beach",
        )
        .await
        .unwrap();

        assert_eq!(result.score, 0.5);
        assert_eq!(result.claims.len(), 2);
        assert_eq!(result.claims[0].verdict, Verdict::Supported);
    }

    #[tokio::test]
    async fn judge_sends_a_json_schema_format_and_think_false() {
        let provider = Arc::new(ScriptedProvider::new(vec![reply(
            json!({"claims": [{"claim": "x", "verdict": "supported"}]}).to_string(),
        )]));
        let llm = Llm::test_with_provider(provider.clone());

        judge(&llm, "test-model", "a prompt", "a caption")
            .await
            .unwrap();

        let sent = &provider.requests()[0];
        assert_eq!(sent.options.think, Some(false));
        assert!(sent.options.format.is_some());
    }

    #[tokio::test]
    async fn malformed_judge_reply_is_a_decode_error() {
        let provider = Arc::new(ScriptedProvider::new(vec![reply("not json")]));
        let llm = Llm::test_with_provider(provider);

        let err = judge(&llm, "test-model", "a prompt", "a caption")
            .await
            .unwrap_err();

        assert!(matches!(err, EvalError::Decode { .. }));
    }

    #[tokio::test]
    async fn caption_step_never_sees_the_prompt() {
        let provider = Arc::new(ScriptedProvider::new(vec![reply("a caption of the image")]));
        let llm = Llm::test_with_provider(provider.clone());

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("image.png");
        image::RgbImage::from_pixel(4, 4, image::Rgb([200, 30, 30]))
            .save(&path)
            .unwrap();

        caption(&llm, "test-model", &path, 512).await.unwrap();

        let sent = &provider.requests()[0];
        let text = sent.messages[0].text();
        assert!(!text.contains("a very specific secret prompt"));
        assert!(text.to_lowercase().contains("describe"));
    }
}
