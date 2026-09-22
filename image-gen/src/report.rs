use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;

use crate::Outcome;
use crate::error::AppError;
use crate::eval::ImageEval;
use crate::eval::rank::Borda;
use crate::generate::UsedParams;

#[derive(Debug, Serialize)]
#[serde(untagged, rename_all_fields = "camelCase")]
pub enum Report {
    Success {
        prompt: PromptReport,
        images: Vec<ImageReport>,
        total_time: f64,
    },
    Failure {
        error: String,
        total_time: f64,
    },
}

#[derive(Debug, Serialize)]
pub struct PromptReport {
    pub original: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rewritten: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ImageReport {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    pub seed: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<ParamsReport>,
    #[serde(flatten)]
    pub eval: ImageEval,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub borda: Option<Borda>,
}

/// The steps/cfg_scale/guidance actually sent for one image — present only when
/// at least one of the three was set (explicitly or by jitter's materialized
/// defaults), so an unjittered copy 0 reports no `params` at all.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParamsReport {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub steps: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cfg_scale: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guidance: Option<f32>,
}

fn params_report(params: &UsedParams) -> Option<ParamsReport> {
    (params.steps.is_some() || params.cfg_scale.is_some() || params.guidance.is_some()).then_some(ParamsReport {
        steps: params.steps,
        cfg_scale: params.cfg_scale,
        guidance: params.guidance,
    })
}

pub fn build(outcome: &Result<Outcome, AppError>, elapsed: Duration) -> Report {
    let total_time = seconds(elapsed);
    match outcome {
        Ok(result) => Report::Success {
            prompt: PromptReport {
                original: result.prompt.original.clone(),
                rewritten: result.prompt.rewritten.clone(),
            },
            images: result
                .images
                .iter()
                .map(|image| ImageReport {
                    path: image.path.as_deref().map(absolutize),
                    seed: image.seed,
                    params: params_report(&image.params),
                    eval: image.eval.clone(),
                    borda: image.borda.clone(),
                })
                .collect(),
            total_time,
        },
        Err(err) => Report::Failure {
            error: err.to_string(),
            total_time,
        },
    }
}

pub fn emit(
    outcome: &Result<Outcome, AppError>,
    elapsed: Duration,
) -> Result<(), serde_json::Error> {
    println!(
        "{}",
        serde_json::to_string_pretty(&build(outcome, elapsed))?
    );
    Ok(())
}

fn absolutize(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

fn seconds(elapsed: Duration) -> f64 {
    (elapsed.as_secs_f64() * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::{Scored, VqaResult};
    use crate::{EffectivePrompt, GeneratedImage};
    use serde_json::json;

    fn outcome(images: Vec<GeneratedImage>, rewritten: Option<&str>) -> Result<Outcome, AppError> {
        Ok(Outcome {
            images,
            prompt: EffectivePrompt {
                original: "a bicycle".to_owned(),
                rewritten: rewritten.map(str::to_owned),
            },
        })
    }

    fn image(path: Option<&str>, seed: i64) -> GeneratedImage {
        GeneratedImage {
            path: path.map(PathBuf::from),
            seed,
            params: UsedParams::default(),
            eval: ImageEval::default(),
            borda: None,
        }
    }

    #[test]
    fn success_shape() {
        let result = outcome(
            vec![
                image(Some("/tmp/path0.png"), 1),
                image(Some("/tmp/path1.png"), 2),
            ],
            None,
        );
        let report = build(&result, Duration::from_micros(123_456_000));
        assert_eq!(
            serde_json::to_value(&report).unwrap(),
            json!({
                "prompt": {"original": "a bicycle"},
                "images": [
                    {"path": "/tmp/path0.png", "seed": 1},
                    {"path": "/tmp/path1.png", "seed": 2},
                ],
                "totalTime": 123.456,
            })
        );
    }

    #[test]
    fn rewritten_prompt_and_scores_are_included() {
        let eval_with_vqa = ImageEval {
            vqa: Some(Scored::Score(VqaResult { score: 0.9 })),
            ..ImageEval::default()
        };
        let result = outcome(
            vec![GeneratedImage {
                path: Some(PathBuf::from("/tmp/shot0.png")),
                seed: 41,
                params: UsedParams::default(),
                eval: eval_with_vqa,
                borda: Some(Borda {
                    vqa: Some(0.0),
                    tit: None,
                    total: 0.0,
                    rank: 1,
                }),
            }],
            Some("a tidy prompt"),
        );

        let report = build(&result, Duration::ZERO);
        let value = serde_json::to_value(&report).unwrap();

        assert_eq!(value["prompt"]["rewritten"], "a tidy prompt");
        assert!((value["images"][0]["vqa"]["score"].as_f64().unwrap() - 0.9).abs() < 1e-6);
        assert_eq!(value["images"][0]["borda"]["rank"], 1);
    }

    #[test]
    fn borda_is_absent_without_eval() {
        let result = outcome(vec![image(Some("/tmp/path0.png"), 1)], None);
        let report = build(&result, Duration::ZERO);
        let value = serde_json::to_value(&report).unwrap();
        assert!(value["images"][0].get("borda").is_none());
    }

    #[test]
    fn params_are_absent_when_unset() {
        let result = outcome(vec![image(Some("/tmp/path0.png"), 1)], None);
        let report = build(&result, Duration::ZERO);
        let value = serde_json::to_value(&report).unwrap();
        assert!(value["images"][0].get("params").is_none());
    }

    #[test]
    fn jittered_params_appear_per_image() {
        let mut jittered = image(Some("/tmp/path1.png"), 2);
        jittered.params = UsedParams {
            steps: Some(22),
            cfg_scale: Some(6.4),
            guidance: Some(3.9),
        };
        let result = outcome(vec![image(Some("/tmp/path0.png"), 1), jittered], None);
        let report = build(&result, Duration::ZERO);
        let value = serde_json::to_value(&report).unwrap();

        assert!(value["images"][0].get("params").is_none());
        assert_eq!(value["images"][1]["params"]["steps"], 22);
        assert!((value["images"][1]["params"]["cfgScale"].as_f64().unwrap() - 6.4).abs() < 1e-6);
        assert!((value["images"][1]["params"]["guidance"].as_f64().unwrap() - 3.9).abs() < 1e-6);
    }

    #[test]
    fn non_durable_image_has_no_path() {
        let result = outcome(vec![image(None, 7)], None);
        let report = build(&result, Duration::ZERO);
        let value = serde_json::to_value(&report).unwrap();
        assert!(value["images"][0].get("path").is_none());
        assert_eq!(value["images"][0]["seed"], 7);
    }

    #[test]
    fn failure_shape() {
        let outcome: Result<Outcome, AppError> = Err(AppError::NoCheckpoint);
        let report = build(&outcome, Duration::from_micros(412_000));
        assert_eq!(
            serde_json::to_value(&report).unwrap(),
            json!({
                "error": AppError::NoCheckpoint.to_string(),
                "totalTime": 0.412,
            })
        );
    }

    #[test]
    fn total_time_rounds_to_milliseconds() {
        assert_eq!(seconds(Duration::from_nanos(123_456_789_012)), 123.457);
    }

    #[test]
    fn relative_path_becomes_absolute() {
        let result = outcome(vec![image(Some("bike.png"), 0)], None);
        let report = build(&result, Duration::ZERO);
        match report {
            Report::Success { images, .. } => {
                let path = images[0].path.as_ref().unwrap();
                assert!(path.is_absolute());
                assert!(path.ends_with("bike.png"));
            }
            Report::Failure { .. } => panic!("expected Success"),
        }
    }
}
