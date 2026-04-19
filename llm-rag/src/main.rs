use std::process::ExitCode;

use clap::Parser;

use llm_rag::cli::{Cli, Command};
use llm_rag::config::{self, Config};
use llm_rag::error::{CliError, ClientError};
use llm_rag::protocol::{Request, Response};
use llm_rag::{client, logging, paths, server};

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

    let role = match cli.command {
        Command::Server => "server",
        Command::Ping => "client",
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
        Command::Server => {
            server::run(cfg, &socket).await?;
            Ok(())
        }
        Command::Ping => {
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
            }
        }
    }
}
