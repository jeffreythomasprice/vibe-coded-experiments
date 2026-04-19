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

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Run as a daemon listening on the Unix socket.
    Server,
    /// Send a ping to the server (auto-starts one if needed).
    Ping,
}
