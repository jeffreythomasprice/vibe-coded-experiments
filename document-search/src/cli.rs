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

        /// OCR every PDF page, ignoring the pdftotext text layer. Use for
        /// scans with a bad embedded text layer, or when garbled output
        /// slips past the decodability check. Works even when
        /// [ocr].enabled = false.
        #[arg(long)]
        force_ocr: bool,

        /// Override [ocr].language for this ingest: tesseract codes joined
        /// with `+` (e.g. "jpn+eng", "jpn_vert+jpn"), or "auto" to detect
        /// the script with tesseract OSD.
        #[arg(long, value_name = "CODES")]
        ocr_language: Option<String>,

        /// Override [summarize].languages for this ingest. Repeat for
        /// multiple trees, e.g. `--summary-lang en --summary-lang source`.
        /// Accepts ISO 639-1 codes or "source" (the detected document
        /// language).
        #[arg(long = "summary-lang", value_name = "CODE")]
        summary_languages: Vec<String>,

        /// Queue the ingest and return immediately without waiting for it
        /// to finish. Use `status` or `queue list` to monitor progress.
        #[arg(short = 'd', long)]
        detach: bool,
    },

    /// Build (or finish) the hierarchical summary tree for an
    /// already-ingested document, without re-extracting or re-embedding it.
    /// Idempotent: groups whose inputs are unchanged are skipped, so this
    /// resumes after an ingest whose summary phase failed, and adds new
    /// trees when given new --summary-lang values. Pass --rebuild to
    /// regenerate every summary instead.
    Summarize {
        /// Path of an already-ingested document.
        path: PathBuf,

        /// Regenerate every summary in the selected language trees, ignoring
        /// the unchanged-input skip. The old tree is kept until the new one
        /// is complete, so a failed or cancelled rebuild changes nothing.
        #[arg(long)]
        rebuild: bool,

        /// Override [summarize].max_depth for this run.
        #[arg(long, value_name = "N")]
        max_depth: Option<usize>,

        /// Override [summarize].languages for this run. Repeatable; accepts
        /// ISO 639-1 codes or "source" (the detected document language).
        #[arg(long = "summary-lang", value_name = "CODE")]
        summary_languages: Vec<String>,

        /// Queue the run and return immediately without waiting for it to
        /// finish. Use `status` or `queue list` to monitor progress.
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

    /// Write a whole ingested document as plain text, markdown and/or PDF,
    /// optionally translated into another language by the LLM. Pass any
    /// combination of --output-txt / --output-md / --output-pdf: they are all
    /// rendered from the same (single) translation. A txt or md path of "-"
    /// means stdout; with no --output-* the text goes to stdout. Formatting
    /// is not preserved: markdown headings and lists are guessed from the
    /// text, and tables or figures come out as text fragments.
    Export {
        /// Path of an already-ingested document.
        path: PathBuf,

        /// Write plain text to this file ("-" for stdout).
        #[arg(long, value_name = "PATH")]
        output_txt: Option<PathBuf>,

        /// Write markdown to this file ("-" for stdout).
        #[arg(long, value_name = "PATH")]
        output_md: Option<PathBuf>,

        /// Write a PDF to this file.
        #[arg(long, value_name = "PATH")]
        output_pdf: Option<PathBuf>,

        /// Translate into this language (ISO 639-1 code, e.g. "en", "ja")
        /// with the configured LLM. Runs as a queued, cancellable job.
        /// Without it the text is exported as written.
        #[arg(long = "to", value_name = "CODE")]
        to: Option<String>,

        /// Override [export].window_chunks: chunks translated per LLM call.
        /// Requires --to.
        #[arg(long, value_name = "N", requires = "to")]
        window_chunks: Option<usize>,

        /// Override [export].overlap_chunks: chunks shared by neighbouring
        /// windows (0 skips the seam-merge pass). Requires --to.
        #[arg(long, value_name = "N", requires = "to")]
        overlap_chunks: Option<usize>,

        /// Run the LLM grammar-edit pass over the text (fixes OCR and
        /// chunking errors such as broken lines and misplaced punctuation;
        /// never rewords). The default comes from [export].edit.
        #[arg(long, overrides_with = "no_edit")]
        edit: bool,

        /// Skip the grammar-edit pass.
        #[arg(long, overrides_with = "edit")]
        no_edit: bool,

        /// Keep every intermediate step (each translated window, seam merge
        /// and edited segment, with its input) in this directory, and skip
        /// steps whose files are already there, so a cancelled or failed
        /// export resumes where it stopped. Reusing a directory built with a
        /// different --window-chunks, --overlap-chunks, model or document is
        /// an error. Without it everything stays in memory.
        #[arg(long, value_name = "DIR")]
        output_intermediates: Option<PathBuf>,

        /// Queue the export and return immediately without waiting for it
        /// to finish. Needs at least one file output (--output-txt/-md/-pdf
        /// with a real path), since there is no client left to receive
        /// stdout. Use `status` or `queue list` to monitor progress.
        #[arg(short = 'd', long)]
        detach: bool,
    },

    /// Print a document's summary tree. With no filters, prints every
    /// summary, top level first. Level 0 holds the leaf summaries (each one
    /// covers a group of chunks); each higher level summarizes the one below.
    /// Use --outline to see the tree's shape (levels, counts, page/byte spans,
    /// parent/child positions) without the text. Bypasses the queue.
    Summaries {
        /// Path of an already-ingested document.
        path: PathBuf,

        /// Which language tree to read (ISO 639-1). Default: the only tree,
        /// else [search].summary_language, else the first resolved
        /// [summarize].languages entry.
        #[arg(long = "lang", value_name = "CODE")]
        language: Option<String>,

        /// Only this level (0 = leaf summaries). Default: every level.
        #[arg(long, value_name = "N")]
        level: Option<u32>,

        #[command(flatten)]
        select: SummarySelectArgs,

        /// Print the tree's structure (one line per summary) instead of the
        /// summary text. Combine with the filters to outline part of it.
        #[arg(long)]
        outline: bool,
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

        /// Text mode: also print each document's tags on a second line.
        /// (In JSON mode tags are always included.)
        #[arg(short = 'v', long)]
        verbose: bool,
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

    /// Manage the Python OCR engines used for Japanese pages (NDLOCR-Lite,
    /// YomiToku). Ingest installs them on first use; this shows what's
    /// installed or installs ahead of time.
    OcrTools {
        #[command(subcommand)]
        action: OcrToolsCommand,
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
pub enum OcrToolsCommand {
    /// Show the install dir, whether uv and git are on PATH, and each
    /// engine's installed version. Bypasses the queue.
    Status,

    /// Install the engines [ocr].japanese_engine uses (both when it is
    /// "tesseract"), skipping any already installed at the configured
    /// version. Runs through the queue and can be cancelled.
    Install {
        /// Delete and reinstall even when already installed.
        #[arg(long)]
        reinstall: bool,
    },
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
#[group(required = false, multiple = false)]
pub struct SummarySelectArgs {
    /// Inclusive range [FIRST LAST] of 0-based positions within --level
    /// (required with this flag).
    #[arg(long, value_names = ["FIRST", "LAST"], num_args = 2, requires = "level")]
    pub index: Option<Vec<u32>>,

    /// Inclusive page range [FIRST LAST] (1-based); every summary whose page
    /// span overlaps it. PDFs only.
    #[arg(long, value_names = ["FIRST", "LAST"], num_args = 2)]
    pub pages: Option<Vec<u32>>,

    /// Inclusive byte range [START END]; every summary whose byte span
    /// overlaps it.
    #[arg(long, value_names = ["START", "END"], num_args = 2)]
    pub bytes: Option<Vec<u64>>,
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
