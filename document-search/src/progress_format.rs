//! Human-readable rendering of `ProgressEnvelope`. Shared by the client
//! spinner, the `status` snapshot printer, and the server's progress log
//! lines so all three use identical wording.

use crate::protocol::{OverallProgress, ProgressEnvelope, ProgressEvent};

/// Only call out "last update Ns ago" once the gap is long enough to suggest
/// a slow step (e.g. a single LLM call) rather than normal event cadence.
const IDLE_NOTE_THRESHOLD_SECS: u64 = 10;

pub(crate) fn format_envelope(env: &ProgressEnvelope) -> String {
    let mut s = format_event(&env.event);
    if let Some(overall) = &env.overall
        && let Some(suffix) = format_overall(overall)
    {
        s.push_str(" — ");
        s.push_str(&suffix);
    }
    s
}

pub(crate) fn format_event(p: &ProgressEvent) -> String {
    match p {
        ProgressEvent::Stage { name } => match name.as_str() {
            "extracting" => "Starting text extraction…".to_string(),
            "ocr" => "Starting OCR fallback…".to_string(),
            "chunking" => "Starting chunking…".to_string(),
            "inserting" => "Committing chunks to DB…".to_string(),
            "summarizing" => "Starting summary tree…".to_string(),
            other => other.to_string(),
        },
        ProgressEvent::Extracting { current, total } => {
            format!("Extracting page {current}/{total}")
        }
        ProgressEvent::Ocr { current, total } => {
            format!("OCR-ing failed page {current}/{total}")
        }
        ProgressEvent::Chunking { current, total } => {
            format!("Chunking text: {current}/{total} (≈ est.)")
        }
        ProgressEvent::Embedding { current, total } => {
            format!("Embedding chunk {current}/{total}")
        }
        ProgressEvent::Summarizing {
            level,
            total_levels,
            current,
            total,
            tree,
            trees,
            language,
        } => {
            let lang = match (language, *trees) {
                (Some(l), n) if n > 1 => format!("{l} {}/{n}, ", tree + 1),
                (Some(l), _) => format!("{l}, "),
                (None, _) => String::new(),
            };
            format!(
                "Extracting group summary ({lang}level {}/{}, group {}/{})",
                level + 1,
                total_levels,
                current,
                total,
            )
        }
    }
}

pub(crate) fn format_overall(o: &OverallProgress) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    // Pre-count phase: surface just elapsed time so a stalled stage is
    // visible without inventing fake step numbers.
    if o.total_steps > 0 {
        let pct = (o.step as f64 / o.total_steps as f64 * 100.0).min(100.0);
        parts.push(format!(
            "{pct:.0}% overall (step {}/{})",
            o.step, o.total_steps
        ));
    }
    if o.elapsed_secs > 0 || !parts.is_empty() {
        parts.push(format!("elapsed {}", fmt_duration(o.elapsed_secs)));
    }
    if let Some(eta) = o.eta_secs {
        parts.push(format!("~{} remaining", fmt_duration(eta)));
    }
    if let Some(idle) = o.idle_secs
        && idle >= IDLE_NOTE_THRESHOLD_SECS
    {
        parts.push(format!("last update {} ago", fmt_duration(idle)));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(", "))
    }
}

/// Compact human duration for the spinner / status line. Drops zero leading
/// units, e.g. 75s → "1m15s", 3725s → "1h2m".
pub(crate) fn fmt_duration(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        if m > 0 {
            format!("{h}h{m}m")
        } else {
            format!("{h}h")
        }
    } else if m > 0 {
        if s > 0 {
            format!("{m}m{s}s")
        } else {
            format!("{m}m")
        }
    } else {
        format!("{s}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overall(step: u64, total_steps: u64, eta: Option<u64>, idle: Option<u64>) -> OverallProgress {
        OverallProgress {
            phase: "summarizing".into(),
            step,
            total_steps,
            elapsed_secs: 2675,
            eta_secs: eta,
            idle_secs: idle,
        }
    }

    #[test]
    fn summarizing_line_has_level_group_percent_and_eta() {
        let env = ProgressEnvelope {
            event: ProgressEvent::Summarizing {
                level: 0,
                total_levels: 3,
                current: 55,
                total: 77,
                tree: 0,
                trees: 1,
                language: None,
            },
            overall: Some(overall(250, 1000, Some(3725), Some(2))),
        };
        assert_eq!(
            format_envelope(&env),
            "Extracting group summary (level 1/3, group 55/77) — \
             25% overall (step 250/1000), elapsed 44m35s, ~1h2m remaining",
        );
    }

    #[test]
    fn summarizing_line_names_language_and_tree() {
        let ev = |trees| ProgressEnvelope {
            event: ProgressEvent::Summarizing {
                level: 1,
                total_levels: 3,
                current: 2,
                total: 4,
                tree: 1,
                trees,
                language: Some("ja".into()),
            },
            overall: None,
        };
        assert_eq!(
            format_envelope(&ev(2)),
            "Extracting group summary (ja 2/2, level 2/3, group 2/4)",
        );
        assert_eq!(
            format_envelope(&ev(1)),
            "Extracting group summary (ja, level 2/3, group 2/4)",
        );
    }

    #[test]
    fn long_idle_gap_is_called_out() {
        let s = format_overall(&overall(1, 10, None, Some(95))).unwrap();
        assert!(s.ends_with("last update 1m35s ago"), "{s}");
    }

    #[test]
    fn pre_count_shows_only_elapsed() {
        let s = format_overall(&overall(0, 0, None, None)).unwrap();
        assert_eq!(s, "elapsed 44m35s");
    }
}
