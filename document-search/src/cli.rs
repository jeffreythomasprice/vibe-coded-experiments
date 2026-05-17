use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::protocol::OutputMode;

#[derive(Parser, Debug)]
#[command(
    name = "document-search",
    version,
    about = "Local document search with Ollama embeddings + turso vector storage"
)]
pub struct Cli {
    /// Path to config.toml. Defaults to ./config.toml or
    /// ~/.config/document-search/config.toml.
    #[arg(short = 'c', long, value_name = "PATH")]
    pub config: Option<PathBuf>,

    /// Output format. `text` (default) is human-readable; `json` makes the
    /// entire stdout a single JSON object suitable for `jq` / scripts.
    #[arg(long, value_enum, default_value_t = OutputMode::Text, global = true)]
    pub output_mode: OutputMode,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Ingest a .txt or .pdf file into the DB. After chunking and embedding,
    /// the hierarchical summary tree is built automatically using the LLM
    /// provider configured under [llm]. Pass --no-summary to skip the summary
    /// phase for this ingest.
    Ingest {
        /// Path to the file to ingest.
        path: PathBuf,

        /// Skip the post-ingest summarization phase. The document is still
        /// chunked and embedded.
        #[arg(long)]
        no_summary: bool,

        /// Override [summarize].max_depth for this ingest's summary phase.
        #[arg(long, value_name = "N")]
        max_depth: Option<usize>,

        /// Override [chunking].chunk_size_bytes for this ingest. The
        /// semantic chunker treats this as a target size and may grow up
        /// to ~1.5× while searching for a structural boundary.
        #[arg(long, value_name = "N")]
        chunk_size: Option<usize>,

        /// Override [chunking].overlap_bytes for this ingest. Must be
        /// smaller than --chunk-size (or the configured default if that
        /// flag is omitted).
        #[arg(long, value_name = "N")]
        overlap: Option<usize>,

        /// Queue the ingest and return immediately without waiting for it
        /// to finish. Use `status` or `queue list` to monitor progress.
        #[arg(short = 'd', long)]
        detach: bool,
    },

    /// Print metadata for a document by path.
    Info {
        /// Path of an already-ingested document.
        path: PathBuf,
    },

    /// Print a range of text from an ingested document. Pick exactly one of
    /// --bytes / --chars / --pages. All ranges are inclusive on both ends.
    Text {
        /// Path of an already-ingested document.
        path: PathBuf,

        #[command(flatten)]
        range: TextRange,
    },

    /// Print the assembled configuration (embedded defaults + user overrides) as TOML.
    PrintConfig,

    /// Run the document-search server in the foreground. Listens on the
    /// Unix socket configured under [server] and auto-exits after the
    /// configured idle timeout.
    Server,

    /// Print the server's queue and currently-running job. Bypasses the
    /// queue.
    Status {
        /// Stream live updates until interrupted with Ctrl-C. The screen is
        /// cleared and redrawn on each refresh in text mode; in JSON mode
        /// one snapshot is printed per line (NDJSON).
        #[arg(short = 'w', long)]
        watch: bool,

        /// Refresh interval in milliseconds (only meaningful with --watch).
        #[arg(long, value_name = "MS", default_value_t = 500, requires = "watch")]
        interval_ms: u64,
    },

    /// Manage the server's job queue.
    Queue {
        #[command(subcommand)]
        action: QueueCommand,
    },

    /// List all ingested documents, plus any in-progress or queued jobs.
    /// Bypasses the queue.
    List {
        /// Filter to documents that have this tag. Repeatable.
        #[arg(short = 't', long = "tag", value_name = "TAG")]
        tags: Vec<String>,

        /// With multiple --tag, require ALL tags to match (default: ANY).
        #[arg(long, requires = "tags")]
        match_all: bool,
    },

    /// Delete an ingested document and all of its chunks/embeddings.
    Delete {
        /// Path of an already-ingested document.
        path: PathBuf,
    },

    /// Manage tags on ingested documents.
    Tag {
        #[command(subcommand)]
        action: TagCommand,
    },

    /// Vector search across ingested documents. Embeds the term via Ollama
    /// and ranks chunks by cosine similarity. Specify exactly one scope:
    /// either --path for a single document or --tag (repeatable).
    Search {
        /// The search term (embedded once via Ollama).
        term: String,

        /// Restrict to a single document by exact ingested path. Mutually
        /// exclusive with --tag/--match-all.
        #[arg(short = 'p', long, value_name = "PATH", conflicts_with_all = ["tags", "match_all"])]
        path: Option<PathBuf>,

        /// Restrict to documents that have this tag. Repeatable.
        /// Default: match ANY given tag. Use --match-all to tighten.
        #[arg(short = 't', long = "tag", value_name = "TAG")]
        tags: Vec<String>,

        /// With multiple --tag, require ALL tags to match (default: ANY).
        #[arg(long, requires = "tags")]
        match_all: bool,

        /// Max chunks to return per in-scope document. Overrides
        /// [search].default_results_per_document.
        #[arg(short = 'n', long, value_name = "N")]
        limit: Option<usize>,

        /// Drop chunks whose similarity (1.0 - cosine_distance) is below
        /// this threshold. Overrides [search].relevancy_cutoff. Range
        /// [0.0, 1.0].
        #[arg(long, value_name = "F")]
        cutoff: Option<f32>,

        /// Return the full chunk text in each result instead of truncating
        /// to ~240 chars. Whitespace is still collapsed.
        #[arg(long)]
        no_truncate: bool,

        /// In addition to chunk-level vector search, also vector-search the
        /// per-document summary tree. Chunks reachable from top-scoring
        /// summaries are merged with the chunk-search hits (deduped, max
        /// similarity wins) and results are grouped by document with a
        /// per-document "region summary" shown alongside its chunks.
        /// Requires that `summarize` has been run for at least one in-scope
        /// document.
        #[arg(long)]
        include_summaries: bool,
    },

    /// Show the most recent tasks the server has executed, oldest first.
    /// Bypasses the queue.
    TaskLog {
        /// Number of most-recent entries to return. Default 10.
        #[arg(short = 'n', long, value_name = "N", default_value_t = 10)]
        limit: usize,
    },
}

#[derive(Subcommand, Debug)]
pub enum QueueCommand {
    /// List queued and currently-running jobs (subset of `status`).
    List,

    /// Remove a queued job by id, or cancel the running job if its id
    /// matches. Accepts the short 8-char prefix shown by `status`/`queue
    /// list`.
    Delete {
        /// Full UUID or any unambiguous prefix of one.
        id: String,
    },

    /// Cancel the current cancellable job (if any) and drop every queued
    /// job.
    Clear,

    /// Scan the DB for orphaned rows left behind by interrupted summarize
    /// runs from older versions and repair them in place.
    Cleanup,
}

#[derive(Subcommand, Debug)]
pub enum TagCommand {
    /// Add one or more tags to a document. Tags are lowercased and trimmed.
    Add {
        /// Path of an already-ingested document.
        path: PathBuf,
        /// Tag(s) to add.
        #[arg(required = true, num_args = 1..)]
        tags: Vec<String>,
    },

    /// Remove one or more tags from a document. Tags not present are a no-op.
    Remove {
        /// Path of an already-ingested document.
        path: PathBuf,
        /// Tag(s) to remove.
        #[arg(required = true, num_args = 1..)]
        tags: Vec<String>,
    },

    /// List all known tags with usage counts.
    List,
}

#[derive(Args, Debug)]
#[group(required = true, multiple = false)]
pub struct TextRange {
    /// Inclusive byte range [START END].
    #[arg(long, value_names = ["START", "END"], num_args = 2)]
    pub bytes: Option<Vec<u64>>,

    /// Inclusive char range [START END].
    #[arg(long, value_names = ["START", "END"], num_args = 2)]
    pub chars: Option<Vec<u64>>,

    /// Inclusive page range [FIRST LAST]; PDFs only.
    #[arg(long, value_names = ["FIRST", "LAST"], num_args = 2)]
    pub pages: Option<Vec<u32>>,
}
