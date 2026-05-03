//! Wire protocol shared by the server and client. Encoded as NDJSON over a
//! Unix socket: the client writes exactly one [`Request`] line, then reads
//! [`Event`] lines until it sees [`Event::Final`].

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

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
    /// Per-chunk chunking progress. `current` is 1-indexed; `total` is an
    /// estimate derived from text length and stride.
    Chunking { current: usize, total: usize },
    /// Per-chunk embedding progress. `current` is 1-indexed.
    Embedding { current: usize, total: usize },
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
