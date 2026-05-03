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
