use std::process::ExitCode;
use std::time::Instant;

use clap::Parser;
use diffusion::cli::{self, Command};
use diffusion::{report, run};

fn main() -> ExitCode {
    let started = Instant::now();
    let cli = cli::Cli::parse();
    let json = matches!(&cli.command, Command::Generate(args) if args.json);

    let outcome = run(cli);
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
