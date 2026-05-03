mod chunking;
mod cli;
mod commands;
mod config;
mod db;
mod error;
mod ingest;
mod logging;
mod ollama;
mod pdf;

use clap::Parser;

use crate::cli::{Cli, Command, TextRange};
use crate::error::Error;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let args = Cli::parse();

    let (config_path, cfg) = config::load(args.config.as_deref())?;
    logging::init()?;

    tracing::info!(
        config_path = %config_path.display(),
        db_path = %cfg.db_path.display(),
        ollama_url = %cfg.ollama.url,
        embedding_model = %cfg.ollama.embedding_model,
        "loaded config",
    );

    let http = reqwest::Client::new();
    let db = db::open(&cfg, &http).await?;

    tracing::info!(
        db_path = %cfg.db_path.display(),
        embedding_model = %db.embedding_model,
        embedding_dimensions = db.embedding_dimensions,
        chunks_table = %db.chunks_table,
        "document-search ready",
    );

    match args.command {
        Command::Ingest { path } => {
            let id = ingest::ingest(&db, &cfg, &http, &path).await?;
            println!("ingested document id {id} from {}", path.display());
        }
        Command::Info { path } => {
            commands::info(&db, &path).await?;
        }
        Command::Text { path, range } => {
            dispatch_text(&db, &path, range).await?;
        }
    }

    Ok(())
}

async fn dispatch_text(
    db: &db::Db,
    path: &std::path::Path,
    range: TextRange,
) -> Result<(), Error> {
    if let Some(v) = range.bytes {
        let (start, end) = pair_u64(&v).map_err(Error::Cli)?;
        commands::text_bytes(db, path, start, end).await?;
    } else if let Some(v) = range.chars {
        let (start, end) = pair_u64(&v).map_err(Error::Cli)?;
        commands::text_chars(db, path, start, end).await?;
    } else if let Some(v) = range.pages {
        let (first, last) = pair_u32(&v).map_err(Error::Cli)?;
        commands::text_pages(db, path, first, last).await?;
    } else {
        // clap's `required = true` group prevents this branch.
        return Err(Error::Cli(
            "expected one of --bytes / --chars / --pages".to_string(),
        ));
    }
    Ok(())
}

fn pair_u64(v: &[u64]) -> Result<(u64, u64), String> {
    match v {
        [a, b] => Ok((*a, *b)),
        _ => Err(format!("expected 2 values, got {}", v.len())),
    }
}

fn pair_u32(v: &[u32]) -> Result<(u32, u32), String> {
    match v {
        [a, b] => Ok((*a, *b)),
        _ => Err(format!("expected 2 values, got {}", v.len())),
    }
}
