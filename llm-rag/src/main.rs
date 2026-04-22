use std::process::ExitCode;

use clap::Parser;

use llm_rag::cli::{Cli, Command, ConversationCmd, DocumentCmd};
use llm_rag::config::{self, Config};
use llm_rag::error::{CliError, ClientError};
use llm_rag::protocol::{
    ConversationSummary, DocumentSearchHit, DocumentSummary, Request, Response,
};
use llm_rag::{client, logging, paths, server, tui};

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    // Config must load before the logger, since `log_dir` lives in config.
    // A failure here predates the logger and takes the stderr-only path.
    let cfg = match config::load(cli.config.as_deref(), cli.secrets.as_deref()) {
        Ok(c) => c,
        Err(err) => return report_and_exit(CliError::from(err)),
    };

    // Hold the guard in `main` so the non_blocking appender flushes on return.
    // `ExitCode` (vs `std::process::exit`) is required so destructors run.
    let _guard = match logging::init(&cfg.log_dir) {
        Ok(g) => g,
        Err(err) => return report_and_exit(CliError::from(err)),
    };

    if let Some(path) = &cfg.secrets.loaded_from_insecure_path {
        tracing::warn!(
            path = %path.display(),
            "secrets file is group/world-accessible; recommend chmod 600"
        );
    }

    let role = match &cli.command {
        Some(Command::Server) => "server",
        Some(Command::Ping) => "client",
        Some(Command::Conversations { .. }) => "client",
        Some(Command::Documents { .. }) => "client",
        Some(Command::Tui) | None => "tui",
    };
    let span = tracing::info_span!("app", pid = std::process::id(), role);
    let _entered = span.enter();

    match dispatch(cli, &cfg).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            tracing::error!(error = %err, "cli error");
            eprintln!("{}", err.to_json_line());
            ExitCode::from(err.exit_code() as u8)
        }
    }
}

fn report_and_exit(err: CliError) -> ExitCode {
    eprintln!("{}", err.to_json_line());
    ExitCode::from(err.exit_code() as u8)
}

async fn dispatch(cli: Cli, cfg: &Config) -> Result<(), CliError> {
    let socket = paths::socket_path(&cfg.socket_dir);

    match cli.command {
        Some(Command::Server) => {
            server::run(cfg, &socket).await?;
            Ok(())
        }
        Some(Command::Ping) => {
            let resp = client::round_trip(
                cfg,
                &socket,
                cli.config.as_deref(),
                cli.secrets.as_deref(),
                Request::Ping,
            )
            .await?;
            match resp {
                Response::Pong => {
                    println!("Pong");
                    Ok(())
                }
                Response::Error { message } => {
                    Err(CliError::Client(ClientError::ServerError { message }))
                }
                other => Err(CliError::Client(ClientError::ServerError {
                    message: format!("unexpected response to Ping: {other:?}"),
                })),
            }
        }
        Some(Command::Tui) | None => {
            tui::run(cfg, &socket, cli.config.as_deref(), cli.secrets.as_deref()).await?;
            Ok(())
        }
        Some(Command::Conversations { action }) => {
            run_conversation_cmd(
                action,
                cfg,
                &socket,
                cli.config.as_deref(),
                cli.secrets.as_deref(),
            )
            .await
        }
        Some(Command::Documents { action }) => {
            run_document_cmd(
                action,
                cfg,
                &socket,
                cli.config.as_deref(),
                cli.secrets.as_deref(),
            )
            .await
        }
    }
}

async fn run_conversation_cmd(
    action: ConversationCmd,
    cfg: &Config,
    socket: &std::path::Path,
    config_override: Option<&std::path::Path>,
    secrets_override: Option<&std::path::Path>,
) -> Result<(), CliError> {
    let req = match &action {
        ConversationCmd::List { tags } => Request::ConversationList {
            tags: tags.clone(),
            text_query: None,
            limit: None,
        },
        ConversationCmd::Delete { id } => Request::ConversationDelete { id: id.clone() },
        ConversationCmd::AddTag { id, tag } => Request::ConversationAddTag {
            id: id.clone(),
            tag: tag.clone(),
        },
        ConversationCmd::RemoveTag { id, tag } => Request::ConversationRemoveTag {
            id: id.clone(),
            tag: tag.clone(),
        },
    };

    let resp = client::round_trip(cfg, socket, config_override, secrets_override, req).await?;

    match (&action, resp) {
        (ConversationCmd::List { .. }, Response::ConversationList { items }) => {
            for item in items {
                println!("{}", format_list_line(&item));
            }
            Ok(())
        }
        (_, Response::Ok) => Ok(()),
        (_, Response::Error { message }) => {
            Err(CliError::Client(ClientError::ServerError { message }))
        }
        (_, other) => Err(CliError::Client(ClientError::ServerError {
            message: format!("unexpected response: {other:?}"),
        })),
    }
}

fn format_list_line(item: &ConversationSummary) -> String {
    let title = item.title.as_deref().unwrap_or("");
    let tags = item.tags.join(",");
    format!("{}\t{}\t{}\t{}", item.id, item.updated_at, title, tags)
}

async fn run_document_cmd(
    action: DocumentCmd,
    cfg: &Config,
    socket: &std::path::Path,
    config_override: Option<&std::path::Path>,
    secrets_override: Option<&std::path::Path>,
) -> Result<(), CliError> {
    match action {
        DocumentCmd::List { tags } => {
            let resp = client::round_trip(
                cfg,
                socket,
                config_override,
                secrets_override,
                Request::DocumentList {
                    tags: tags.clone(),
                    limit: None,
                },
            )
            .await?;
            match resp {
                Response::DocumentList { items } => {
                    for item in items {
                        println!("{}", format_document_line(&item));
                    }
                    Ok(())
                }
                Response::Error { message } => {
                    Err(CliError::Client(ClientError::ServerError { message }))
                }
                other => Err(CliError::Client(ClientError::ServerError {
                    message: format!("unexpected response: {other:?}"),
                })),
            }
        }
        DocumentCmd::Delete { id } => simple_ok(
            client::round_trip(
                cfg,
                socket,
                config_override,
                secrets_override,
                Request::DocumentDelete { id },
            )
            .await?,
        ),
        DocumentCmd::AddTag { id, tag } => simple_ok(
            client::round_trip(
                cfg,
                socket,
                config_override,
                secrets_override,
                Request::DocumentAddTag { id, tag },
            )
            .await?,
        ),
        DocumentCmd::RemoveTag { id, tag } => simple_ok(
            client::round_trip(
                cfg,
                socket,
                config_override,
                secrets_override,
                Request::DocumentRemoveTag { id, tag },
            )
            .await?,
        ),
        DocumentCmd::New { path, tags } => {
            // Resolve to an absolute path so the server finds it regardless
            // of its own CWD.
            let abs = match path.canonicalize() {
                Ok(p) => p,
                Err(err) => {
                    return Err(CliError::Client(ClientError::ServerError {
                        message: format!("resolving {}: {err}", path.display()),
                    }));
                }
            };
            let mut terminal: Option<Response> = None;
            client::document_create_stream(
                cfg,
                socket,
                config_override,
                secrets_override,
                Request::DocumentCreate {
                    path: abs.to_string_lossy().into_owned(),
                    tags,
                },
                |frame| match &frame {
                    Response::DocumentCreateStart {
                        document_id,
                        total_chunks,
                    } => {
                        eprintln!("ingesting id={document_id} chunks={total_chunks}");
                    }
                    Response::DocumentCreateProgress {
                        completed,
                        in_flight,
                        total,
                        ..
                    } => {
                        eprintln!("  {completed}/{total} complete · {in_flight} in flight");
                    }
                    Response::DocumentCreateDone { .. } | Response::Error { .. } => {
                        terminal = Some(frame);
                    }
                    _ => {}
                },
            )
            .await?;
            match terminal {
                Some(Response::DocumentCreateDone {
                    document_id,
                    chunks,
                }) => {
                    println!("{document_id}\t{chunks} chunks");
                    Ok(())
                }
                Some(Response::Error { message }) => {
                    Err(CliError::Client(ClientError::ServerError { message }))
                }
                _ => Err(CliError::Client(ClientError::ServerError {
                    message: "ingest ended without terminal frame".to_string(),
                })),
            }
        }
        DocumentCmd::Search { query, tags, limit } => {
            let resp = client::round_trip(
                cfg,
                socket,
                config_override,
                secrets_override,
                Request::DocumentSearch {
                    query,
                    tags,
                    limit: Some(limit),
                },
            )
            .await?;
            match resp {
                Response::DocumentSearch { results } => {
                    for hit in results {
                        println!("{}", format_search_line(&hit));
                    }
                    Ok(())
                }
                Response::Error { message } => {
                    Err(CliError::Client(ClientError::ServerError { message }))
                }
                other => Err(CliError::Client(ClientError::ServerError {
                    message: format!("unexpected response: {other:?}"),
                })),
            }
        }
    }
}

// `CliError` carries several per-layer error variants and is ~128 bytes.
// This helper is short enough that clippy flags it; the fn is small and
// only called from the document command dispatch, so the cost is trivial.
#[allow(clippy::result_large_err)]
fn simple_ok(resp: Response) -> Result<(), CliError> {
    match resp {
        Response::Ok => Ok(()),
        Response::Error { message } => Err(CliError::Client(ClientError::ServerError { message })),
        other => Err(CliError::Client(ClientError::ServerError {
            message: format!("unexpected response: {other:?}"),
        })),
    }
}

fn format_document_line(item: &DocumentSummary) -> String {
    let tags = item.tags.join(",");
    format!("{}\t{}\t{}\t{}", item.id, item.created_at, item.path, tags)
}

fn format_search_line(hit: &DocumentSearchHit) -> String {
    let snippet: String = hit.content.chars().take(100).collect();
    let snippet = snippet.replace('\n', " ");
    format!(
        "{}\t{:.4}\t{}\t{}",
        hit.document_id, hit.distance, hit.path, snippet
    )
}
