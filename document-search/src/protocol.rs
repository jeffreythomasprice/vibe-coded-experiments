//! Wire protocol shared by the server and client. Encoded as NDJSON over a
//! Unix socket: the client writes exactly one [`Request`] line, then reads
//! [`Event`] lines until it sees [`Event::Final`].

use std::path::{Path, PathBuf};

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
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        force_ocr: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ocr_language: Option<String>,
        /// Empty means "use [summarize].languages".
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        summary_languages: Vec<String>,
        /// Server hint: after sending the initial `QueuedAhead`, immediately
        /// close the connection with `Final{ok=true}`. The worker still runs
        /// the job; the client just doesn't wait.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        detach: bool,
    },
    /// Build (or top up) the summary tree for an already-ingested document.
    /// Idempotent: groups whose inputs are unchanged are skipped, unless
    /// `rebuild` is set.
    Summarize {
        path: PathBuf,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        rebuild: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_depth: Option<usize>,
        /// Empty means "use [summarize].languages".
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        summary_languages: Vec<String>,
        /// Same as `Ingest::detach`.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        detach: bool,
    },
    Info { path: PathBuf },
    Text { path: PathBuf, range: TextRangeReq },
    /// Read a document's summary tree: its shape, and the text of any
    /// level / index range / page or byte range. Bypasses the queue.
    Summaries {
        path: PathBuf,
        /// `None` picks the only tree, else the preferred summary language.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        language: Option<String>,
        /// `None` means every level.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        level: Option<u32>,
        #[serde(default)]
        select: SummarySelect,
        /// Structure only: omit summary text.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        outline: bool,
    },
    /// Write a whole ingested document as txt / markdown / PDF, optionally
    /// translated. Without `to_language` it only reads, so it bypasses the
    /// queue; with it, it is a cancellable worker job.
    Export {
        path: PathBuf,
        /// Every format to produce from one (possibly translated) text.
        outputs: Vec<ExportOutput>,
        /// Target language (ISO 639-1). `None` keeps the document as written.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to_language: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        window_chunks: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        overlap_chunks: Option<usize>,
    },
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
        /// Text mode only: print each document's tags on a second line.
        /// Ignored in JSON mode, where tags are always included.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        verbose: bool,
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
    /// Install dir, uv/git presence, and installed engine versions.
    /// Bypasses the queue.
    OcrToolsStatus,
    /// Install (or with `reinstall`, rebuild) the Japanese OCR engines.
    /// Runs through the worker so it never races an ingest's own install.
    OcrToolsInstall {
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        reinstall: bool,
    },
    /// Show the most recent rows from the persistent `task_log` table.
    /// Bypasses the queue.
    TaskLog {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<usize>,
    },
}

fn default_trees() -> usize {
    1
}

fn default_status_interval_ms() -> u64 {
    500
}

/// How a queued or running job will change the document at its path. Used
/// by `info` to report work that hasn't landed yet.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PendingEffect {
    Create,
    Update,
    Delete,
}

impl PendingEffect {
    pub fn as_str(self) -> &'static str {
        match self {
            PendingEffect::Create => "create",
            PendingEffect::Update => "update",
            PendingEffect::Delete => "delete",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum TextRangeReq {
    Bytes { start: u64, end: u64 },
    Chars { start: u64, end: u64 },
    Pages { first: u32, last: u32 },
}

/// Which summaries of a tree `summaries` returns. All ranges are inclusive;
/// `Index` is 0-based within one level, `Pages` 1-based, and `Pages` /
/// `Bytes` keep every summary whose span overlaps the range.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum,
)]
#[serde(rename_all = "kebab-case")]
#[clap(rename_all = "kebab-case")]
pub enum ExportFormat {
    Txt,
    Md,
    Pdf,
}

impl ExportFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            ExportFormat::Txt => "txt",
            ExportFormat::Md => "md",
            ExportFormat::Pdf => "pdf",
        }
    }
}

/// One requested export file. `path: None` returns the txt / markdown to
/// the client as output text (PDF always needs a path).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportOutput {
    pub format: ExportFormat,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum SummarySelect {
    #[default]
    All,
    Index { first: u32, last: u32 },
    Pages { first: u32, last: u32 },
    Bytes { start: u64, end: u64 },
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
                force_ocr,
                ocr_language,
                summary_languages,
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
                if *force_ocr {
                    s.push_str(" --force-ocr");
                }
                if let Some(l) = ocr_language {
                    s.push_str(&format!(" --ocr-language {l}"));
                }
                for l in summary_languages {
                    s.push_str(&format!(" --summary-lang {l}"));
                }
                s
            }
            Request::Summarize {
                path,
                rebuild,
                max_depth,
                summary_languages,
                detach: _,
            } => {
                let mut s = format!("summarize {}", path.display());
                if *rebuild {
                    s.push_str(" --rebuild");
                }
                if let Some(d) = max_depth {
                    s.push_str(&format!(" --max-depth {d}"));
                }
                for l in summary_languages {
                    s.push_str(&format!(" --summary-lang {l}"));
                }
                s
            }
            Request::Info { path } => format!("info {}", path.display()),
            Request::Text { path, .. } => format!("text {}", path.display()),
            Request::Summaries { path, .. } => format!("summaries {}", path.display()),
            Request::Export {
                path,
                outputs,
                to_language,
                ..
            } => {
                let mut s = format!("export {}", path.display());
                if let Some(l) = to_language {
                    s.push_str(&format!(" --to {l}"));
                }
                for o in outputs {
                    let target = o
                        .path
                        .as_ref()
                        .map_or("-".to_string(), |p| p.display().to_string());
                    s.push_str(&format!(" --output-{} {target}", o.format.as_str()));
                }
                s
            }
            Request::PrintConfig => "print-config".to_string(),
            Request::Status { watch: false, .. } => "status".to_string(),
            Request::Status { watch: true, .. } => "status --watch".to_string(),
            Request::QueueList => "queue list".to_string(),
            Request::QueueDelete { id } => format!("queue delete {id}"),
            Request::QueueClear => "queue clear".to_string(),
            Request::QueueCleanup => "queue cleanup".to_string(),
            Request::List {
                tags, match_all, ..
            } => {
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
            Request::OcrToolsStatus => "ocr-tools status".to_string(),
            Request::OcrToolsInstall { reinstall: false } => "ocr-tools install".to_string(),
            Request::OcrToolsInstall { reinstall: true } => {
                "ocr-tools install --reinstall".to_string()
            }
            Request::TaskLog { .. } => "task-log".to_string(),
        }
    }

    /// The document path this request will change, and how. `None` for
    /// read-only requests (info, text, search, …) and non-document jobs.
    pub fn pending_effect(&self) -> Option<(PendingEffect, &Path)> {
        match self {
            Request::Ingest { path, .. } => Some((PendingEffect::Create, path)),
            Request::Summarize { path, .. }
            | Request::TagAdd { path, .. }
            | Request::TagRemove { path, .. } => Some((PendingEffect::Update, path)),
            Request::Delete { path } => Some((PendingEffect::Delete, path)),
            _ => None,
        }
    }

    /// For fire-and-forget requests (`--detach`): the verb and path to echo
    /// back once the job is queued. `None` means the client waits for the
    /// job to finish as usual.
    pub fn detached_target(&self) -> Option<(&'static str, &PathBuf)> {
        match self {
            Request::Ingest { path, detach: true, .. } => Some(("ingest", path)),
            Request::Summarize { path, detach: true, .. } => Some(("summarize", path)),
            _ => None,
        }
    }

    /// Returns `(task_name, path, tags)` for the persistent task_log row this
    /// request will produce when the worker runs it. Inline-handled variants
    /// (status/info/list/queue-*/tag-list/task-log) never reach the worker, so
    /// they return `None` for the whole tuple — callers shouldn't be logging
    /// them.
    pub fn task_metrics(&self) -> Option<(&'static str, Option<String>, Option<String>)> {
        match self {
            Request::Ingest { path, .. } => {
                Some(("ingest", Some(path.display().to_string()), None))
            }
            Request::Summarize { path, .. } => {
                Some(("summarize", Some(path.display().to_string()), None))
            }
            Request::Text { path, .. } => {
                Some(("text", Some(path.display().to_string()), None))
            }
            Request::Export {
                path,
                to_language: Some(l),
                ..
            } => Some(("export", Some(path.display().to_string()), Some(l.clone()))),
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
            Request::OcrToolsInstall { .. } => Some(("ocr-tools-install", None, None)),
            Request::Status { .. }
            | Request::Info { .. }
            | Request::Summaries { .. }
            | Request::Export { .. }
            | Request::OcrToolsStatus
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
    /// group_size, and max_depth. `tree` (0-indexed) / `trees` locate this
    /// tree among the per-language trees of the run; `language` is the
    /// tree's output language. All three default so older peers still parse.
    Summarizing {
        level: usize,
        total_levels: usize,
        current: usize,
        total: usize,
        #[serde(default)]
        tree: usize,
        #[serde(default = "default_trees")]
        trees: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        language: Option<String>,
    },
    /// `export --to`: translating window `current` (1-indexed, emitted
    /// before the window starts) of `total`. `seams` is how many seam-merge
    /// calls will follow, so the tracker can count them from the start.
    Translating {
        current: usize,
        total: usize,
        #[serde(default)]
        seams: usize,
    },
    /// `export --to`: merging the seam between windows `current` and
    /// `current + 1` (1-indexed, emitted before the merge starts).
    Seaming { current: usize, total: usize },
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
    /// Seconds since the job last emitted a real progress event. Envelopes
    /// are re-rendered on a heartbeat and at `status` time, so this is how a
    /// long single step (e.g. one slow LLM call) shows up as a gap.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idle_secs: Option<u64>,
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
    fn summaries_round_trip_and_defaults() {
        let env = RequestEnvelope {
            output_mode: OutputMode::Json,
            request: Request::Summaries {
                path: PathBuf::from("/a.pdf"),
                language: Some("en".into()),
                level: Some(1),
                select: SummarySelect::Pages { first: 3, last: 9 },
                outline: true,
            },
        };
        let s = serde_json::to_string(&env).unwrap();
        let back: RequestEnvelope = serde_json::from_str(&s).unwrap();
        match back.request {
            Request::Summaries { path, language, level, select, outline } => {
                assert_eq!(path, PathBuf::from("/a.pdf"));
                assert_eq!(language.as_deref(), Some("en"));
                assert_eq!(level, Some(1));
                assert_eq!(select, SummarySelect::Pages { first: 3, last: 9 });
                assert!(outline);
            }
            other => panic!("expected Summaries, got {other:?}"),
        }

        let bare: RequestEnvelope =
            serde_json::from_str(r#"{"cmd":"summaries","path":"/a.pdf"}"#).unwrap();
        match bare.request {
            Request::Summaries { language, level, select, outline, .. } => {
                assert_eq!(language, None);
                assert_eq!(level, None);
                assert_eq!(select, SummarySelect::All);
                assert!(!outline);
            }
            other => panic!("expected Summaries, got {other:?}"),
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
    fn old_ingest_request_parses_with_new_fields_defaulted() {
        let s = r#"{"cmd":"ingest","path":"/x.pdf","no_summary":true}"#;
        let env: RequestEnvelope = serde_json::from_str(s).unwrap();
        match env.request {
            Request::Ingest {
                no_summary,
                force_ocr,
                ocr_language,
                summary_languages,
                ..
            } => {
                assert!(no_summary);
                assert!(!force_ocr);
                assert_eq!(ocr_language, None);
                assert!(summary_languages.is_empty());
            }
            _ => panic!("expected Ingest variant"),
        }
    }

    #[test]
    fn old_summarizing_event_parses_as_single_tree() {
        let s = r#"{"stage":"summarizing","level":1,"total_levels":3,"current":2,"total":4}"#;
        match serde_json::from_str::<ProgressEvent>(s) {
            Ok(ProgressEvent::Summarizing { tree, trees, language, .. }) => {
                assert_eq!((tree, trees, language), (0, 1, None));
            }
            other => panic!("expected Summarizing, got {other:?}"),
        }
    }

    #[test]
    fn ingest_label_lists_new_flags() {
        let req = Request::Ingest {
            path: "/x.pdf".into(),
            no_summary: false,
            max_depth: None,
            chunk_size: None,
            overlap: None,
            force_ocr: true,
            ocr_language: Some("jpn+eng".into()),
            summary_languages: vec!["en".into(), "source".into()],
            detach: false,
        };
        assert_eq!(
            req.label(),
            "ingest /x.pdf --force-ocr --ocr-language jpn+eng --summary-lang en --summary-lang source"
        );
    }

    #[test]
    fn pending_effect_covers_mutating_requests() {
        let p = || PathBuf::from("/x.pdf");
        let effect = |r: Request| r.pending_effect().map(|(e, _)| e);
        assert_eq!(
            effect(Request::Delete { path: p() }),
            Some(PendingEffect::Delete)
        );
        assert_eq!(
            effect(Request::TagAdd { path: p(), tags: vec![] }),
            Some(PendingEffect::Update)
        );
        assert_eq!(
            effect(Request::Summarize {
                path: p(),
                rebuild: false,
                max_depth: None,
                summary_languages: vec![],
                detach: false,
            }),
            Some(PendingEffect::Update)
        );
        assert_eq!(effect(Request::Info { path: p() }), None);
        assert_eq!(
            effect(Request::Export {
                path: p(),
                outputs: vec![ExportOutput { format: ExportFormat::Md, path: None }],
                to_language: Some("en".into()),
                window_chunks: None,
                overlap_chunks: None,
            }),
            None
        );
        assert_eq!(
            effect(Request::Text {
                path: p(),
                range: TextRangeReq::Bytes { start: 0, end: 1 },
            }),
            None
        );
    }

    #[test]
    fn export_round_trips_and_only_translation_is_logged() {
        let req = Request::Export {
            path: PathBuf::from("/x.pdf"),
            outputs: vec![ExportOutput {
                format: ExportFormat::Pdf,
                path: Some(PathBuf::from("/out.pdf")),
            }],
            to_language: Some("en".into()),
            window_chunks: Some(3),
            overlap_chunks: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"cmd\":\"export\""), "{json}");
        assert!(json.contains("\"format\":\"pdf\""), "{json}");
        let back: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(back.task_metrics().map(|m| m.0), Some("export"));
        let plain = Request::Export {
            path: PathBuf::from("/x.pdf"),
            outputs: vec![ExportOutput { format: ExportFormat::Txt, path: None }],
            to_language: None,
            window_chunks: None,
            overlap_chunks: None,
        };
        assert!(plain.task_metrics().is_none());
    }

    #[test]
    fn translating_without_seams_still_parses() {
        let ev: ProgressEvent =
            serde_json::from_str(r#"{"stage":"translating","current":1,"total":3}"#).unwrap();
        assert!(matches!(ev, ProgressEvent::Translating { seams: 0, .. }));
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
                        idle_secs: None,
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
                idle_secs: None,
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
                tree: 0,
                trees: 1,
                language: None,
            },
            overall: None,
        };
        let s = serde_json::to_string(&env).unwrap();
        assert!(s.contains("\"total_levels\":3"));
        assert!(s.contains("\"level\":0"));
        let back: ProgressEnvelope = serde_json::from_str(&s).unwrap();
        match back.event {
            ProgressEvent::Summarizing { level, total_levels, current, total, .. } => {
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
