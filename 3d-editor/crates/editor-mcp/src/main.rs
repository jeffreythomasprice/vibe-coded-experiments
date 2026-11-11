mod server;
mod tools;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use clap::Parser;
use editor_cmd::results::FileInfoResult;
use editor_proto::{Client, ClientError, Config, ConfigError, Policy};
use rmcp::ServiceExt;
use serde_json::json;
use server::EditorServer;

#[derive(Parser)]
#[command(about = "MCP server (stdio) for the scene editor; a client of editor-server")]
struct Args {
    /// Scene file to attach to; without one, join the server's focused session
    file: Option<PathBuf>,
    /// Create the file if it does not exist (needs --allow-mutate)
    #[arg(long)]
    create: bool,
    /// Enable mutating tools
    #[arg(long)]
    allow_mutate: bool,
    /// Enable destructive tools (implies --allow-mutate)
    #[arg(long)]
    allow_destructive: bool,
    /// Config file (default: $EDITOR_CONFIG or ~/.config/editor/config.toml)
    #[arg(long)]
    config: Option<PathBuf>,
}

#[derive(Debug, thiserror::Error)]
enum MainError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Client(#[from] ClientError),
    #[error("{0} does not exist (pass --create to start a new scene there)")]
    Missing(PathBuf),
    #[error("attaching to a session: {0}")]
    Attach(String),
    #[error("mcp transport: {0}")]
    Transport(String),
}

fn attach(client: &Client, args: &Args) -> Result<(), MainError> {
    let request = match &args.file {
        Some(path) => {
            if !path.exists() && !args.create {
                return Err(MainError::Missing(path.clone()));
            }
            json!({ "op": "session.attach", "args": { "path": path, "create": args.create } })
        }
        None => json!({ "op": "session.attach", "args": {} }),
    };
    let reply = client.request_value(request)?;
    if reply.is_ok() {
        return Ok(());
    }
    let (message, hint) = reply
        .error()
        .map_or(("refused", ""), |e| (e.message.as_str(), e.hint.as_str()));
    Err(MainError::Attach(
        format!("{message} {hint}").trim().to_string(),
    ))
}

async fn serve(args: Args) -> Result<(), MainError> {
    let config = match &args.config {
        Some(path) => Config::load_from(path)?,
        None => Config::load()?,
    };
    let destructive = args.allow_destructive || config.mcp.allow_destructive;
    let mutate = args.allow_mutate || config.mcp.allow_mutate || destructive;
    let policy = Policy::new(mutate, destructive);

    let client = Client::connect(&config, "mcp", mutate, destructive)?;
    client.discard_events();
    attach(&client, &args)?;
    let client = Arc::new(client);

    let server = EditorServer::new(client.clone(), policy);
    let running = server
        .serve(rmcp::transport::stdio())
        .await
        .map_err(|e| MainError::Transport(e.to_string()))?;
    running
        .waiting()
        .await
        .map_err(|e| MainError::Transport(e.to_string()))?;
    if let Ok(reply) = client.request_value(json!({ "op": "file.info" }))
        && reply.result::<FileInfoResult>().is_ok_and(|i| i.dirty)
    {
        eprintln!(
            "editor-mcp: the session has unsaved changes (editor-server keeps it; file_save was not called)"
        );
    }
    Ok(())
}

#[tokio::main]
async fn main() -> ExitCode {
    match serve(Args::parse()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("editor-mcp: {e}");
            ExitCode::FAILURE
        }
    }
}
