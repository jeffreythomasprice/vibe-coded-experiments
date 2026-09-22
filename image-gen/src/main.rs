use std::process::ExitCode;
use std::time::Instant;

use clap::Parser;
use image_gen::cli::{self, Command};
use image_gen::{report, run};

#[tokio::main]
async fn main() -> ExitCode {
    let started = Instant::now();
    let cli = cli::Cli::parse();
    let json = matches!(&cli.command, Command::Generate(args) if args.json);

    let outcome = run(cli).await;
    let code = match &outcome {
        Ok(_) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    };

    if !json {
        return code;
    }
    match report::emit(&outcome, started.elapsed()) {
        Ok(()) => code,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}
