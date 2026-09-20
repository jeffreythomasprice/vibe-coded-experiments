use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;

use crate::error::AppError;

#[derive(Debug, Serialize)]
#[serde(untagged, rename_all_fields = "camelCase")]
pub enum Report {
    Success {
        paths: Vec<PathBuf>,
        total_time: f64,
    },
    Failure {
        error: String,
        total_time: f64,
    },
}

pub fn build(outcome: &Result<Vec<PathBuf>, AppError>, elapsed: Duration) -> Report {
    let total_time = seconds(elapsed);
    match outcome {
        Ok(paths) => Report::Success {
            paths: paths.iter().map(|path| absolutize(path)).collect(),
            total_time,
        },
        Err(err) => Report::Failure {
            error: err.to_string(),
            total_time,
        },
    }
}

pub fn emit(
    outcome: &Result<Vec<PathBuf>, AppError>,
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
    use serde_json::json;

    #[test]
    fn success_shape() {
        let outcome: Result<Vec<PathBuf>, AppError> = Ok(vec![
            PathBuf::from("/tmp/path0.png"),
            PathBuf::from("/tmp/path1.png"),
        ]);
        let report = build(&outcome, Duration::from_micros(123_456_000));
        assert_eq!(
            serde_json::to_value(&report).unwrap(),
            json!({
                "paths": ["/tmp/path0.png", "/tmp/path1.png"],
                "totalTime": 123.456,
            })
        );
    }

    #[test]
    fn failure_shape() {
        let outcome: Result<Vec<PathBuf>, AppError> = Err(AppError::NoCheckpoint);
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
        let outcome: Result<Vec<PathBuf>, AppError> = Ok(vec![PathBuf::from("bike.png")]);
        let report = build(&outcome, Duration::ZERO);
        match report {
            Report::Success { paths, .. } => {
                assert!(paths[0].is_absolute());
                assert!(paths[0].ends_with("bike.png"));
            }
            Report::Failure { .. } => panic!("expected Success"),
        }
    }
}
