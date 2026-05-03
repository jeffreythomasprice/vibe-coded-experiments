use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "document-search",
    version,
    about = "Local document search with Ollama embeddings + turso vector storage"
)]
pub struct Cli {
    /// Path to config.toml. Defaults to ./config.toml or
    /// ~/.config/document-search/config.toml.
    #[arg(long, value_name = "PATH")]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Ingest a .txt or .pdf file into the DB.
    Ingest {
        /// Path to the file to ingest.
        path: PathBuf,
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
    Status,

    /// List all ingested documents, plus any in-progress or queued jobs.
    /// Bypasses the queue.
    List {
        /// Filter to documents that have this tag. Repeatable.
        #[arg(long = "tag", value_name = "TAG")]
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

    /// Cancel the currently-running ingest, if any. Bypasses the queue.
    Cancel,

    /// Manage tags on ingested documents.
    Tag {
        #[command(subcommand)]
        action: TagCommand,
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
