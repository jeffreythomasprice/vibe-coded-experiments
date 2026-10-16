mod chunking;
mod cleanup;
mod cli;
mod client;
mod commands;
mod config;
mod db;
mod error;
mod ingest;
mod language;
mod llm;
mod logging;
mod ocr_ja;
mod ocr_merge;
mod ocr_tools;
mod ollama;
mod pdf;
mod proc;
mod progress_format;
mod protocol;
mod server;
mod summaries;
mod summarize;

use clap::Parser;

use crate::cli::{
    Cli, Command, OcrToolsCommand, QueueCommand, SummarySelectArgs, TagCommand, TextRange,
};
use crate::error::Error;
use crate::protocol::{OutputMode, Request, RequestEnvelope, SummarySelect, TextRangeReq};

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
            let mut request = command_to_request(other)?;
            absolutize_paths(&mut request);
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
        Command::Ingest {
            path,
            no_summary,
            max_depth,
            chunk_size,
            overlap,
            force_ocr,
            ocr_language,
            summary_languages,
            detach,
        } => Ok(Request::Ingest {
            path,
            no_summary,
            max_depth,
            chunk_size,
            overlap,
            force_ocr,
            ocr_language,
            summary_languages,
            detach,
        }),
        Command::Summarize {
            path,
            rebuild,
            max_depth,
            summary_languages,
            detach,
        } => Ok(Request::Summarize {
            path,
            rebuild,
            max_depth,
            summary_languages,
            detach,
        }),
        Command::Info { path } => Ok(Request::Info { path }),
        Command::Text { path, range } => Ok(Request::Text {
            path,
            range: text_range_to_proto(range)?,
        }),
        Command::Summaries {
            path,
            language,
            level,
            select,
            outline,
        } => Ok(Request::Summaries {
            path,
            language,
            level,
            select: summary_select_to_proto(select)?,
            outline,
        }),
        Command::PrintConfig => Ok(Request::PrintConfig),
        Command::Status { watch, interval_ms } => Ok(Request::Status { watch, interval_ms }),
        Command::Queue { action } => match action {
            QueueCommand::List => Ok(Request::QueueList),
            QueueCommand::Delete { id } => Ok(Request::QueueDelete { id }),
            QueueCommand::Clear => Ok(Request::QueueClear),
            QueueCommand::Cleanup => Ok(Request::QueueCleanup),
        },
        Command::List {
            tags,
            match_all,
            verbose,
        } => Ok(Request::List {
            tags,
            match_all,
            verbose,
        }),
        Command::Delete { path } => Ok(Request::Delete { path }),
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
        Command::OcrTools { action } => match action {
            OcrToolsCommand::Status => Ok(Request::OcrToolsStatus),
            OcrToolsCommand::Install { reinstall } => Ok(Request::OcrToolsInstall { reinstall }),
        },
        Command::TaskLog { limit } => Ok(Request::TaskLog { limit: Some(limit) }),
        Command::Server => unreachable!("Server is dispatched directly"),
    }
}

/// Resolve relative document paths against the client's CWD. The server
/// is a long-lived process with whatever CWD the first client gave it, so
/// it can't resolve them itself. Lexical only: the file need not exist (e.g.
/// `info` on a path a queued ingest will create).
fn absolutize_paths(req: &mut Request) {
    let path = match req {
        Request::Ingest { path, .. }
        | Request::Summarize { path, .. }
        | Request::Info { path }
        | Request::Text { path, .. }
        | Request::Summaries { path, .. }
        | Request::Delete { path }
        | Request::TagAdd { path, .. }
        | Request::TagRemove { path, .. }
        | Request::Search { path: Some(path), .. } => path,
        _ => return,
    };
    if let Ok(abs) = std::path::absolute(&*path) {
        *path = abs;
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

fn summary_select_to_proto(select: SummarySelectArgs) -> Result<SummarySelect, Error> {
    if let Some(v) = select.index {
        let (first, last) = pair_u32(&v).map_err(Error::Cli)?;
        Ok(SummarySelect::Index { first, last })
    } else if let Some(v) = select.pages {
        let (first, last) = pair_u32(&v).map_err(Error::Cli)?;
        Ok(SummarySelect::Pages { first, last })
    } else if let Some(v) = select.bytes {
        let (start, end) = pair_u64(&v).map_err(Error::Cli)?;
        Ok(SummarySelect::Bytes { start, end })
    } else {
        Ok(SummarySelect::All)
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
