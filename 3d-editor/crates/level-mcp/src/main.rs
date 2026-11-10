mod server;
mod tools;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use level_cmd::Session;
use level_core::Document;
use level_host::{Host, Policy};
use rmcp::ServiceExt;
use server::LevelServer;

#[derive(Parser)]
#[command(about = "MCP server for the level editor command API (stdio)")]
struct Args {
    /// Level file to edit; without one the document lives only in memory
    file: Option<PathBuf>,
    /// Create the file if it does not exist
    #[arg(long)]
    create: bool,
    /// Enable mutating tools
    #[arg(long)]
    allow_mutate: bool,
    /// Enable destructive tools (implies --allow-mutate)
    #[arg(long)]
    allow_destructive: bool,
}

#[derive(Debug, thiserror::Error)]
enum MainError {
    #[error(transparent)]
    File(#[from] level_fmt::FileError),
    #[error("{0} does not exist (pass --create to start a new level there)")]
    Missing(PathBuf),
    #[error("mcp transport: {0}")]
    Transport(String),
}

fn open_session(args: &Args) -> Result<Session, MainError> {
    let Some(path) = &args.file else {
        return Ok(Session::new(Document::new("untitled")));
    };
    if !path.exists() {
        if !args.create {
            return Err(MainError::Missing(path.clone()));
        }
        let name = path
            .file_name()
            .map(|n| {
                n.to_string_lossy()
                    .trim_end_matches(".level.toml")
                    .to_string()
            })
            .unwrap_or_default();
        let doc = Document::new(name);
        level_fmt::save_file(path, &doc)?;
        return Ok(Session::with_path(doc, path.clone()));
    }
    Ok(Session::with_path(
        level_fmt::load_file(path)?,
        path.clone(),
    ))
}

async fn serve(args: Args) -> Result<(), MainError> {
    let session = open_session(&args)?;
    let policy = Policy::new(args.allow_mutate, args.allow_destructive);
    let server = LevelServer::new(Host::new(session, policy));
    let host = server.host();
    let running = server
        .serve(rmcp::transport::stdio())
        .await
        .map_err(|e| MainError::Transport(e.to_string()))?;
    running
        .waiting()
        .await
        .map_err(|e| MainError::Transport(e.to_string()))?;
    let host = host.lock().unwrap_or_else(|p| p.into_inner());
    if host.session().is_dirty() {
        eprintln!("level-mcp: exiting with unsaved changes (file_save was not called)");
    }
    Ok(())
}

#[tokio::main]
async fn main() -> ExitCode {
    match serve(Args::parse()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("level-mcp: {e}");
            ExitCode::FAILURE
        }
    }
}
