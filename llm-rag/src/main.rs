use std::process::ExitCode;

use clap::Parser;

use llm_rag::cli::{Cli, Command, ConversationCmd};
use llm_rag::config::{self, Config};
use llm_rag::error::{CliError, ClientError};
use llm_rag::protocol::{ConversationSummary, Request, Response};
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
