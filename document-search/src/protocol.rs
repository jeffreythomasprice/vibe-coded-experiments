//! Wire protocol shared by the server and client. Encoded as NDJSON over a
//! Unix socket: the client writes exactly one [`Request`] line, then reads
//! [`Event`] lines until it sees [`Event::Final`].

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Output mode chosen on the client and forwarded to the server so each
/// command can format its payload accordingly.
#[derive(
    Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, clap::ValueEnum,
)]
#[serde(rename_all = "kebab-case")]
#[clap(rename_all = "kebab-case")]
pub enum OutputMode {
    #[default]
    Text,
    Json,
}

/// Wire envelope: every request from the client carries the chosen output
/// mode alongside the actual `Request`. `#[serde(flatten)]` keeps the
/// existing `cmd` discriminator at the top level next to `output_mode`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestEnvelope {
    #[serde(default)]
    pub output_mode: OutputMode,
    #[serde(flatten)]
    pub request: Request,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "kebab-case")]
pub enum Request {
    Ingest {
        path: PathBuf,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        no_summary: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_depth: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        chunk_size: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        overlap: Option<usize>,
        /// Server hint: after sending the initial `QueuedAhead`, immediately
        /// close the connection with `Final{ok=true}`. The worker still runs
        /// the job; the client just doesn't wait.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        detach: bool,
    },
    Info { path: PathBuf },
    Text { path: PathBuf, range: TextRangeReq },
    PrintConfig,
    Status {
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        watch: bool,
        /// Refresh interval in milliseconds when `watch` is true. Ignored
        /// otherwise. Defaults to 500.
        #[serde(default = "default_status_interval_ms")]
        interval_ms: u64,
    },
    /// Subset of `Status`: just the queue + currently-running job. Bypasses
    /// the worker queue.
    QueueList,
    /// Remove a queued job by id (or 8-char prefix). If the id matches the
    /// currently-running cancellable job, cancels it.
    QueueDelete { id: String },
    /// Cancel current (if cancellable) and drop every queued job.
    QueueClear,
    /// Scan the DB for orphaned rows from interrupted summarize runs and
    /// repair them in place.
    QueueCleanup,
    List {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tags: Vec<String>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        match_all: bool,
    },
    Delete { path: PathBuf },
    TagAdd { path: PathBuf, tags: Vec<String> },
    TagRemove { path: PathBuf, tags: Vec<String> },
    TagList,
    Search {
        term: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        path: Option<PathBuf>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tags: Vec<String>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        match_all: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cutoff: Option<f32>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        no_truncate: bool,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        include_summaries: bool,
    },
    /// Show the most recent rows from the persistent `task_log` table.
    /// Bypasses the queue.
    TaskLog {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<usize>,
    },
}

fn default_status_interval_ms() -> u64 {
    500
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum TextRangeReq {
    Bytes { start: u64, end: u64 },
    Chars { start: u64, end: u64 },
    Pages { first: u32, last: u32 },
}

impl Request {
    /// Short label shown in queue/status output.
    pub fn label(&self) -> String {
        match self {
            Request::Ingest {
                path,
                no_summary,
                max_depth,
                chunk_size,
                overlap,
                detach: _,
            } => {
                let mut s = format!("ingest {}", path.display());
                if *no_summary {
                    s.push_str(" --no-summary");
                }
                if let Some(d) = max_depth {
                    s.push_str(&format!(" --max-depth {d}"));
                }
                if let Some(n) = chunk_size {
                    s.push_str(&format!(" --chunk-size {n}"));
                }
                if let Some(n) = overlap {
                    s.push_str(&format!(" --overlap {n}"));
                }
                s
            }
            Request::Info { path } => format!("info {}", path.display()),
            Request::Text { path, .. } => format!("text {}", path.display()),
            Request::PrintConfig => "print-config".to_string(),
            Request::Status { watch: false, .. } => "status".to_string(),
            Request::Status { watch: true, .. } => "status --watch".to_string(),
            Request::QueueList => "queue list".to_string(),
            Request::QueueDelete { id } => format!("queue delete {id}"),
            Request::QueueClear => "queue clear".to_string(),
            Request::QueueCleanup => "queue cleanup".to_string(),
            Request::List { tags, match_all } => {
                if tags.is_empty() {
                    "list".to_string()
                } else {
                    let mode = if *match_all { "all" } else { "any" };
                    format!("list --tag {} ({})", tags.join(","), mode)
                }
            }
            Request::Delete { path } => format!("delete {}", path.display()),
            Request::TagAdd { path, tags } => {
                format!("tag add {} [{}]", path.display(), tags.join(","))
            }
            Request::TagRemove { path, tags } => {
                format!("tag remove {} [{}]", path.display(), tags.join(","))
            }
            Request::TagList => "tag list".to_string(),
            Request::Search {
                term,
                path,
                tags,
                match_all,
                ..
            } => {
                let scope = if let Some(p) = path {
                    format!(" --path {}", p.display())
                } else if !tags.is_empty() {
                    let mode = if *match_all { "all" } else { "any" };
                    format!(" --tag {} ({})", tags.join(","), mode)
                } else {
                    String::new()
                };
                format!("search {:?}{}", term, scope)
            }
            Request::TaskLog { .. } => "task-log".to_string(),
        }
    }

    /// Returns `(task_name, path, tags)` for the persistent task_log row this
    /// request will produce when the worker runs it. Inline-handled variants
    /// (status/list/queue-*/tag-list/task-log) never reach the worker, so
    /// they return `None` for the whole tuple — callers shouldn't be logging
    /// them.
    pub fn task_metrics(&self) -> Option<(&'static str, Option<String>, Option<String>)> {
        match self {
            Request::Ingest { path, .. } => {
                Some(("ingest", Some(path.display().to_string()), None))
            }
            Request::Info { path } => {
                Some(("info", Some(path.display().to_string()), None))
            }
            Request::Text { path, .. } => {
                Some(("text", Some(path.display().to_string()), None))
            }
            Request::PrintConfig => Some(("print-config", None, None)),
            Request::Delete { path } => {
                Some(("delete", Some(path.display().to_string()), None))
            }
            Request::TagAdd { path, tags } => Some((
                "tag-add",
                Some(path.display().to_string()),
                Some(tags.join(",")),
            )),
            Request::TagRemove { path, tags } => Some((
                "tag-remove",
                Some(path.display().to_string()),
                Some(tags.join(",")),
            )),
            Request::Search { path, tags, .. } => Some((
                "search",
                path.as_ref().map(|p| p.display().to_string()),
                if tags.is_empty() { None } else { Some(tags.join(",")) },
            )),
            Request::Status { .. }
            | Request::QueueList
            | Request::QueueDelete { .. }
            | Request::QueueClear
            | Request::QueueCleanup
            | Request::List { .. }
            | Request::TagList
            | Request::TaskLog { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Event {
    /// How many jobs are still ahead of this one in the queue.
    QueuedAhead { ahead: u64 },
    /// Worker has picked up this job and started executing it.
    Started,
    /// Real-time progress update for the running job. Carries the raw stage
    /// event plus a server-derived `OverallProgress` envelope so the spinner
    /// and the `status` snapshot can render identical wording.
    Progress(ProgressEnvelope),
    /// Captured stdout to print on the client.
    Output { text: String },
    /// Response payload for `status`.
    StatusSnapshot(StatusSnapshot),
    /// Last event in any response. `ok=false` => error.
    Final {
        ok: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "stage", rename_all = "kebab-case")]
pub enum ProgressEvent {
    /// Named pipeline stage (e.g. "extracting", "chunking", "inserting").
    Stage { name: String },
    /// Per-page PDF text extraction progress. `current` is 1-indexed.
    Extracting { current: u32, total: u32 },
    /// Per-page OCR fallback progress, fired only for pages whose pdftotext
    /// output failed the decodability check. `total` is the count of bad
    /// pages (not the document's total page count); `current` is 1-indexed.
    Ocr { current: u32, total: u32 },
    /// Per-chunk chunking progress. `current` is 1-indexed; `total` is an
    /// estimate derived from text length and stride.
    Chunking { current: usize, total: usize },
    /// Per-chunk embedding progress. `current` is 1-indexed.
    Embedding { current: usize, total: usize },
    /// Per-summary build progress within a single tree level. `level` is the
    /// depth being built (0 = chunks → first summaries). `current` is
    /// 1-indexed. `total_levels` is an upper-bound estimate of how many
    /// summary levels the tree will end up with, derived from chunk count,
    /// group_size, and max_depth.
    Summarizing {
        level: usize,
        total_levels: usize,
        current: usize,
        total: usize,
    },
}

/// Wraps a raw `ProgressEvent` with a server-derived view of how far the
/// whole job has progressed. Same payload is used by the streaming spinner
/// and the `status` snapshot, so both sides see identical wording.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressEnvelope {
    pub event: ProgressEvent,
    /// Absent before the server has enough information to estimate totals
    /// (typically just the very first `Stage` event before any counts).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overall: Option<OverallProgress>,
}

/// Coarse whole-job progress estimate, computed server-side from accumulated
/// per-phase counts and timing samples. Step counts are weighted units; the
/// weights reflect typical per-step cost (e.g. one LLM summary call is ~10x
/// an embedding call). Useful for direction-of-travel, not precision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverallProgress {
    /// Current high-level phase label.
    pub phase: String,
    /// Weighted units completed so far across all known phases.
    pub step: u64,
    /// Weighted units total — grows as later phases reveal their sizes.
    pub total_steps: u64,
    /// Seconds since the job started running.
    pub elapsed_secs: u64,
    /// Estimated seconds remaining. `None` until at least one phase has
    /// accumulated enough samples to extrapolate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eta_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusSnapshot {
    pub uptime_secs: u64,
    pub current: Option<CurrentJob>,
    pub queued: Vec<QueuedJob>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentJob {
    /// Full UUID of the running job. `queue delete` accepts an 8-char prefix.
    pub id: String,
    pub label: String,
    pub running_secs: u64,
    /// Latest progress envelope emitted by the running job, if any. Absent
    /// for jobs that don't emit progress (info, text, list, …) or for
    /// ingests that haven't reached their first progress emission yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<ProgressEnvelope>,
    /// Seconds since `progress` was recorded. Only present when `progress` is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress_age_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedJob {
    pub id: String,
    pub label: String,
    pub queued_secs: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_default_text_mode_round_trips() {
        let env = RequestEnvelope {
            output_mode: OutputMode::default(),
            request: Request::Status { watch: false, interval_ms: 500 },
        };
        let s = serde_json::to_string(&env).unwrap();
        // Top-level keys: output_mode + cmd discriminator.
        assert!(s.contains("\"output_mode\":\"text\""));
        assert!(s.contains("\"cmd\":\"status\""));
        let back: RequestEnvelope = serde_json::from_str(&s).unwrap();
        assert_eq!(back.output_mode, OutputMode::Text);
        assert!(matches!(back.request, Request::Status { watch: false, .. }));
    }

    #[test]
    fn envelope_json_mode_round_trips_search_with_no_truncate() {
        let env = RequestEnvelope {
            output_mode: OutputMode::Json,
            request: Request::Search {
                term: "hello".into(),
                path: None,
                tags: vec!["legal".into()],
                match_all: false,
                limit: Some(7),
                cutoff: Some(0.4),
                no_truncate: true,
                include_summaries: false,
            },
        };
        let s = serde_json::to_string(&env).unwrap();
        assert!(s.contains("\"output_mode\":\"json\""));
        assert!(s.contains("\"cmd\":\"search\""));
        assert!(s.contains("\"term\":\"hello\""));
        assert!(s.contains("\"no_truncate\":true"));

        let back: RequestEnvelope = serde_json::from_str(&s).unwrap();
        assert_eq!(back.output_mode, OutputMode::Json);
        match back.request {
            Request::Search {
                term,
                no_truncate,
                limit,
                cutoff,
                ..
            } => {
                assert_eq!(term, "hello");
                assert!(no_truncate);
                assert_eq!(limit, Some(7));
                assert_eq!(cutoff, Some(0.4));
            }
            _ => panic!("expected Search variant"),
        }
    }

    #[test]
    fn missing_output_mode_defaults_to_text() {
        // Backwards compatibility: an old client serializing only the `Request`
        // shape (no output_mode field) should deserialize as `text`.
        let s = r#"{"cmd":"status"}"#;
        let env: RequestEnvelope = serde_json::from_str(s).unwrap();
        assert_eq!(env.output_mode, OutputMode::Text);
        match env.request {
            Request::Status { watch, interval_ms } => {
                assert!(!watch);
                assert_eq!(interval_ms, 500);
            }
            _ => panic!("expected Status variant"),
        }
    }

    #[test]
    fn current_job_without_progress_round_trips() {
        let snap = StatusSnapshot {
            uptime_secs: 10,
            current: Some(CurrentJob {
                id: "abc".into(),
                label: "ingest foo.pdf".into(),
                running_secs: 5,
                progress: None,
                progress_age_secs: None,
            }),
            queued: vec![],
        };
        let s = serde_json::to_string(&snap).unwrap();
        // Absent fields must not appear in the wire form.
        assert!(!s.contains("progress"), "unexpected progress key: {s}");
        let back: StatusSnapshot = serde_json::from_str(&s).unwrap();
        let c = back.current.unwrap();
        assert!(c.progress.is_none());
        assert!(c.progress_age_secs.is_none());
    }

    #[test]
    fn current_job_with_progress_round_trips() {
        let snap = StatusSnapshot {
            uptime_secs: 10,
            current: Some(CurrentJob {
                id: "abc".into(),
                label: "ingest foo.pdf".into(),
                running_secs: 12,
                progress: Some(ProgressEnvelope {
                    event: ProgressEvent::Embedding { current: 47, total: 120 },
                    overall: Some(OverallProgress {
                        phase: "embedding".into(),
                        step: 94,
                        total_steps: 700,
                        elapsed_secs: 12,
                        eta_secs: Some(180),
                    }),
                }),
                progress_age_secs: Some(3),
            }),
            queued: vec![],
        };
        let s = serde_json::to_string(&snap).unwrap();
        assert!(s.contains("\"progress\""));
        assert!(s.contains("\"stage\":\"embedding\""));
        assert!(s.contains("\"progress_age_secs\":3"));
        assert!(s.contains("\"overall\""));
        assert!(s.contains("\"eta_secs\":180"));
        let back: StatusSnapshot = serde_json::from_str(&s).unwrap();
        let c = back.current.unwrap();
        let env = c.progress.expect("progress present");
        match env.event {
            ProgressEvent::Embedding { current, total } => {
                assert_eq!(current, 47);
                assert_eq!(total, 120);
            }
            other => panic!("expected Embedding, got {other:?}"),
        }
        let overall = env.overall.expect("overall present");
        assert_eq!(overall.phase, "embedding");
        assert_eq!(overall.step, 94);
        assert_eq!(overall.total_steps, 700);
        assert_eq!(overall.eta_secs, Some(180));
        assert_eq!(c.progress_age_secs, Some(3));
    }

    #[test]
    fn overall_progress_skips_eta_when_none() {
        let env = ProgressEnvelope {
            event: ProgressEvent::Stage { name: "extracting".into() },
            overall: Some(OverallProgress {
                phase: "extracting".into(),
                step: 0,
                total_steps: 0,
                elapsed_secs: 0,
                eta_secs: None,
            }),
        };
        let s = serde_json::to_string(&env).unwrap();
        assert!(!s.contains("eta_secs"), "unexpected eta_secs in {s}");
        let back: ProgressEnvelope = serde_json::from_str(&s).unwrap();
        assert!(back.overall.unwrap().eta_secs.is_none());
    }

    #[test]
    fn summarizing_event_carries_total_levels() {
        let env = ProgressEnvelope {
            event: ProgressEvent::Summarizing {
                level: 0,
                total_levels: 3,
                current: 55,
                total: 77,
            },
            overall: None,
        };
        let s = serde_json::to_string(&env).unwrap();
        assert!(s.contains("\"total_levels\":3"));
        assert!(s.contains("\"level\":0"));
        let back: ProgressEnvelope = serde_json::from_str(&s).unwrap();
        match back.event {
            ProgressEvent::Summarizing { level, total_levels, current, total } => {
                assert_eq!(level, 0);
                assert_eq!(total_levels, 3);
                assert_eq!(current, 55);
                assert_eq!(total, 77);
            }
            other => panic!("expected Summarizing, got {other:?}"),
        }
    }

    #[test]
    fn current_job_legacy_payload_deserializes() {
        // Older server (no progress field) must still parse — guards client
        // code that may run against a not-yet-upgraded server.
        let s = r#"{"uptime_secs":1,"current":{"id":"x","label":"y","running_secs":1},"queued":[]}"#;
        let back: StatusSnapshot = serde_json::from_str(s).unwrap();
        assert!(back.current.unwrap().progress.is_none());
    }

    #[test]
    fn search_missing_no_truncate_defaults_false() {
        let s = r#"{"output_mode":"text","cmd":"search","term":"x","tags":["t"]}"#;
        let env: RequestEnvelope = serde_json::from_str(s).unwrap();
        match env.request {
            Request::Search { no_truncate, .. } => assert!(!no_truncate),
            _ => panic!("expected Search variant"),
        }
    }
}
