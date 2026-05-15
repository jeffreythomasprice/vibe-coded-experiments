mod chunking;
mod cli;
mod client;
mod commands;
mod config;
mod db;
mod error;
mod ingest;
mod llm;
mod logging;
mod ollama;
mod pdf;
mod protocol;
mod server;
mod summarize;

use clap::Parser;

use crate::cli::{Cli, Command, TagCommand, TextRange};
use crate::error::Error;
use crate::protocol::{OutputMode, Request, RequestEnvelope, TextRangeReq};

#[tokio::main]
async fn main() {
    let args = Cli::parse();
    let output_mode = args.output_mode;
    if let Err(e) = run(args).await {
        // Errors that originated server-side already have a complete
        // human-readable message; surface them directly without re-wrapping.
        let msg = match &e {
            Error::Client(crate::client::ClientError::Server(msg)) => msg.clone(),
            _ => e.to_string(),
        };
        match output_mode {
            OutputMode::Json => {
                // For Server-originated errors, the client has already
                // printed the JSON envelope on stdout. Avoid double-printing.
                let already_printed =
                    matches!(&e, Error::Client(crate::client::ClientError::Server(_)));
                if !already_printed {
                    let env = serde_json::json!({"ok": false, "error": msg});
                    println!("{env}");
                }
            }
            OutputMode::Text => {
                eprintln!("error: {msg}");
            }
        }
        std::process::exit(1);
    }
}

async fn run(args: Cli) -> Result<(), Error> {
    let (config_path, cfg) = config::load(args.config.as_deref())?;
    let output_mode = args.output_mode;

    match args.command {
        Command::Server => {
            // Server logs to stderr and the shared file. Clients log to the
            // shared file only — see logging::init.
            logging::init(logging::Role::Server, &cfg.logging)?;
            let config_path_display = match &config_path {
                Some(p) => p.display().to_string(),
                None => "<embedded defaults>".to_string(),
            };
            tracing::info!(
                config_path = %config_path_display,
                db_path = %cfg.db_path.display(),
                ollama_url = %cfg.ollama.url,
                embedding_model = %cfg.ollama.embedding_model,
                socket_path = %cfg.server.socket_path.display(),
                "loaded config",
            );
            server::run(cfg).await?;
            Ok(())
        }
        other => {
            logging::init(logging::Role::Client, &cfg.logging)?;
            let request = command_to_request(other)?;
            let envelope = RequestEnvelope {
                output_mode,
                request,
            };
            client::run(&cfg, envelope, args.config.as_deref())
                .await
                .map_err(Error::from)
        }
    }
}

fn command_to_request(cmd: Command) -> Result<Request, Error> {
    match cmd {
        Command::Ingest { path } => Ok(Request::Ingest { path }),
        Command::Info { path } => Ok(Request::Info { path }),
        Command::Text { path, range } => Ok(Request::Text {
            path,
            range: text_range_to_proto(range)?,
        }),
        Command::PrintConfig => Ok(Request::PrintConfig),
        Command::Status => Ok(Request::Status),
        Command::List { tags, match_all } => Ok(Request::List { tags, match_all }),
        Command::Delete { path } => Ok(Request::Delete { path }),
        Command::Cancel => Ok(Request::Cancel),
        Command::Tag { action } => match action {
            TagCommand::Add { path, tags } => Ok(Request::TagAdd { path, tags }),
            TagCommand::Remove { path, tags } => Ok(Request::TagRemove { path, tags }),
            TagCommand::List => Ok(Request::TagList),
        },
        Command::Search {
            term,
            path,
            tags,
            match_all,
            limit,
            cutoff,
            no_truncate,
            include_summaries,
        } => Ok(Request::Search {
            term,
            path,
            tags,
            match_all,
            limit,
            cutoff,
            no_truncate,
            include_summaries,
        }),
        Command::Summarize { path, max_depth } => Ok(Request::Summarize { path, max_depth }),
        Command::Server => unreachable!("Server is dispatched directly"),
    }
}

fn text_range_to_proto(range: TextRange) -> Result<TextRangeReq, Error> {
    if let Some(v) = range.bytes {
        let (start, end) = pair_u64(&v).map_err(Error::Cli)?;
        Ok(TextRangeReq::Bytes { start, end })
    } else if let Some(v) = range.chars {
        let (start, end) = pair_u64(&v).map_err(Error::Cli)?;
        Ok(TextRangeReq::Chars { start, end })
    } else if let Some(v) = range.pages {
        let (first, last) = pair_u32(&v).map_err(Error::Cli)?;
        Ok(TextRangeReq::Pages { first, last })
    } else {
        Err(Error::Cli(
            "expected one of --bytes / --chars / --pages".to_string(),
        ))
    }
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
