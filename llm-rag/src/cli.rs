use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "llm-rag", version, about = "LLM-with-RAG client/server")]
pub struct Cli {
    /// Override the config file search path.
    #[arg(long, global = true, value_name = "PATH")]
    pub config: Option<PathBuf>,

    /// Override the secrets file search path. If set, the file MUST exist;
    /// without an override, a missing secrets file is silently OK.
    #[arg(long, global = true, value_name = "PATH")]
    pub secrets: Option<PathBuf>,

    /// Subcommand to run. If omitted, the interactive TUI launches.
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Run as a daemon listening on the Unix socket.
    Server,
    /// Send a ping to the server (auto-starts one if needed).
    Ping,
    /// Launch the interactive TUI (default when no subcommand is given).
    Tui,
    /// Manage stored conversations.
    Conversations {
        #[command(subcommand)]
        action: ConversationCmd,
    },
    /// Manage ingested documents (chunks + embeddings).
    Documents {
        #[command(subcommand)]
        action: DocumentCmd,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConversationCmd {
    /// List conversations, optionally filtered to those carrying ALL of the
    /// given tags. One line per conversation:
    ///   <id>\t<updated_at>\t<title>\t<tag1,tag2,…>
    List {
        /// Match conversations that have this tag. Repeat for ALL-of.
        #[arg(long = "tag", value_name = "TAG")]
        tags: Vec<String>,
    },
    /// Delete a conversation by id (cascades to its messages and tag links).
    Delete { id: String },
    /// Add a tag to a conversation.
    AddTag { id: String, tag: String },
    /// Remove a tag from a conversation.
    RemoveTag { id: String, tag: String },
}

#[derive(Subcommand, Debug)]
pub enum DocumentCmd {
    /// List documents, optionally filtered to those carrying ALL of the given
    /// tags. One line per document:
    ///   <id>\t<created_at>\t<path>\t<tag1,tag2,…>
    List {
        /// Match documents that have this tag. Repeat for ALL-of.
        #[arg(long = "tag", value_name = "TAG")]
        tags: Vec<String>,
    },
    /// Delete a document by id (cascades to its chunks and tag links).
    Delete { id: i64 },
    /// Ingest a plain-text file: chunk, embed, and persist. Path is opened on
    /// the server (client + server share a filesystem).
    New {
        /// Path to the text file (ASCII or UTF-8). Extension is ignored.
        path: PathBuf,
        /// Attach this tag on ingest. Repeatable.
        #[arg(long = "tag", value_name = "TAG")]
        tags: Vec<String>,
    },
    /// Add a tag to a document.
    AddTag { id: i64, tag: String },
    /// Remove a tag from a document.
    RemoveTag { id: i64, tag: String },
    /// Vector search against stored chunks. Embeds the query and returns the
    /// K nearest chunks, optionally filtered to documents with ALL of the
    /// given tags. One line per hit:
    ///   <doc_id>\t<distance>\t<path>\t<snippet>
    Search {
        /// Search text; embedded against the same model used for ingest.
        query: String,
        /// Restrict to documents carrying this tag. Repeat for ALL-of.
        #[arg(long = "tag", value_name = "TAG")]
        tags: Vec<String>,
        /// Maximum hits to return.
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
}
