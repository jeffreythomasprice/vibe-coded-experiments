pub mod rank;
pub mod tit;
pub mod vqa;

use std::io::Cursor;
use std::path::Path;

use serde::Serialize;
use thiserror::Error;

use crate::cli::EvalMetric;
use crate::llm::{Image, Llm, LlmError};
pub use rank::Borda;
pub use tit::TitResult;
pub use vqa::VqaResult;

#[derive(Debug, Error)]
pub enum EvalError {
    #[error(transparent)]
    Llm(#[from] LlmError),

    #[error("failed to downscale image for evaluation: {0}")]
    Image(#[from] image::ImageError),

    #[error("failed to parse the judge's structured response: {source}")]
    Decode {
        #[source]
        source: serde_json::Error,
    },

    #[error("neither \"yes\" nor \"no\" appeared among the top tokens: {tokens}")]
    NoAnswerToken { tokens: String },

    #[error("judge returned zero claims")]
    NoClaims,
}

/// One metric's outcome, or the error that stopped it — kept per metric per image
/// so one failing score does not discard an otherwise-successful `--copies N` run.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum Scored<T> {
    Score(T),
    Failed { error: String },
}

impl<T> Scored<T> {
    fn from_result(result: Result<T, EvalError>) -> Self {
        match result {
            Ok(value) => Scored::Score(value),
            Err(err) => Scored::Failed {
                error: err.to_string(),
            },
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ImageEval {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vqa: Option<Scored<VqaResult>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tit: Option<Scored<TitResult>>,
}

pub struct EvalConfig<'a> {
    pub metrics: &'a [EvalMetric],
    pub vqa_model: &'a str,
    pub caption_model: &'a str,
    pub judge_model: &'a str,
    pub max_px: u32,
}

/// Runs every requested metric against one image, logging progress per metric
/// since a `--copies N --eval tit` run can take minutes and would otherwise sit
/// silent. `docs/evaluation.md` notes VQAScore is the weaker choice once a prompt
/// is long enough to trigger `--rewrite`; that tradeoff is documented, not enforced
/// here — both metrics still run if both are requested.
pub fn run(llm: &Llm, config: &EvalConfig, prompt: &str, path: &Path) -> ImageEval {
    let mut result = ImageEval::default();
    for metric in config.metrics {
        match metric {
            EvalMetric::Vqa => {
                tracing::info!(path = %path.display(), metric = "vqa", "scoring image");
                let outcome = vqa::score(llm, config.vqa_model, prompt, path, config.max_px);
                if let Err(err) = &outcome {
                    tracing::warn!(path = %path.display(), metric = "vqa", error = %err, "scoring failed");
                }
                result.vqa = Some(Scored::from_result(outcome));
            }
            EvalMetric::Tit => {
                tracing::info!(path = %path.display(), metric = "tit", "scoring image");
                let outcome = tit::score(
                    llm,
                    config.caption_model,
                    config.judge_model,
                    prompt,
                    path,
                    config.max_px,
                );
                if let Err(err) = &outcome {
                    tracing::warn!(path = %path.display(), metric = "tit", error = %err, "scoring failed");
                }
                result.tit = Some(Scored::from_result(outcome));
            }
        }
    }
    result
}

/// Downscales the image at `path` to at most `max_px` per side and re-encodes it
/// as an in-memory PNG. Ollama's own docs call image token count the single
/// biggest latency lever for a vision request.
pub(crate) fn load_scaled(path: &Path, max_px: u32) -> Result<Image, EvalError> {
    let decoded = image::open(path)?;
    let scaled = decoded.thumbnail(max_px, max_px);
    let mut bytes = Vec::new();
    scaled.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)?;
    Ok(Image::from_bytes(bytes)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scored_serializes_success_as_score_shape() {
        let scored: Scored<VqaResult> = Scored::from_result(Ok(VqaResult { score: 0.9 }));
        let value = serde_json::to_value(&scored).unwrap();
        assert!(
            (value["score"].as_f64().unwrap() - 0.9).abs() < 1e-6,
            "value was {value}"
        );
        assert!(value.get("error").is_none());
    }

    #[test]
    fn scored_serializes_failure_as_error_shape() {
        let scored: Scored<VqaResult> = Scored::from_result(Err(EvalError::NoClaims));
        let value = serde_json::to_value(&scored).unwrap();
        assert_eq!(value["error"], "judge returned zero claims");
    }

    #[test]
    fn image_eval_omits_unrequested_metrics() {
        let eval = ImageEval {
            vqa: Some(Scored::Score(VqaResult { score: 1.0 })),
            tit: None,
        };
        let value = serde_json::to_value(&eval).unwrap();
        assert!(value.get("tit").is_none());
        assert_eq!(value["vqa"]["score"], 1.0);
    }
}
