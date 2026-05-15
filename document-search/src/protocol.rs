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
    Ingest { path: PathBuf },
    Info { path: PathBuf },
    Text { path: PathBuf, range: TextRangeReq },
    PrintConfig,
    Status,
    List {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tags: Vec<String>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        match_all: bool,
    },
    Delete { path: PathBuf },
    Cancel,
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
    Summarize {
        path: PathBuf,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_depth: Option<usize>,
    },
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
            Request::Ingest { path } => format!("ingest {}", path.display()),
            Request::Info { path } => format!("info {}", path.display()),
            Request::Text { path, .. } => format!("text {}", path.display()),
            Request::PrintConfig => "print-config".to_string(),
            Request::Status => "status".to_string(),
            Request::List { tags, match_all } => {
                if tags.is_empty() {
                    "list".to_string()
                } else {
                    let mode = if *match_all { "all" } else { "any" };
                    format!("list --tag {} ({})", tags.join(","), mode)
                }
            }
            Request::Delete { path } => format!("delete {}", path.display()),
            Request::Cancel => "cancel".to_string(),
            Request::TagAdd { path, tags } => {
                format!("tag add {} [{}]", path.display(), tags.join(","))
            }
            Request::TagRemove { path, tags } => {
                format!("tag remove {} [{}]", path.display(), tags.join(","))
            }
            Request::TagList => "tag list".to_string(),
            Request::Summarize { path, max_depth } => match max_depth {
                Some(d) => format!("summarize {} --max-depth {}", path.display(), d),
                None => format!("summarize {}", path.display()),
            },
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
    /// Real-time progress update for the running job.
    Progress(ProgressEvent),
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
    /// 1-indexed.
    Summarizing {
        level: usize,
        current: usize,
        total: usize,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusSnapshot {
    pub uptime_secs: u64,
    pub current: Option<CurrentJob>,
    pub queued: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentJob {
    pub label: String,
    pub running_secs: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_default_text_mode_round_trips() {
        let env = RequestEnvelope {
            output_mode: OutputMode::default(),
            request: Request::Status,
        };
        let s = serde_json::to_string(&env).unwrap();
        // Top-level keys: output_mode + cmd discriminator.
        assert!(s.contains("\"output_mode\":\"text\""));
        assert!(s.contains("\"cmd\":\"status\""));
        let back: RequestEnvelope = serde_json::from_str(&s).unwrap();
        assert_eq!(back.output_mode, OutputMode::Text);
        assert!(matches!(back.request, Request::Status));
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
