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
