use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use exalted::error::{ValidationError, ValidationReport};
use exalted::render::character_to_markdown;
use exalted::Character;

#[derive(Parser)]
#[command(name = "exalted", version, about = "Exalted 2e character sheet tools")]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Render a character JSON file as markdown to stdout (or `-o FILE`).
    Render {
        /// Path to the character JSON file.
        file: PathBuf,
        /// Optional output path. If omitted, writes to stdout.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Validate a character against chargen and XP rules.
    ///
    /// Exit code is 0 if there are no errors (notes are informational),
    /// non-zero if any validation errors are found or the file cannot be
    /// parsed.
    Validate {
        /// Path to the character JSON file.
        file: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Cmd::Render { file, output } => run_render(file, output),
        Cmd::Validate { file } => run_validate(file),
    }
}

fn load_character(path: &PathBuf) -> Result<Character, String> {
    let bytes = fs::read(path)
        .map_err(|e| format!("could not read {}: {}", path.display(), e))?;
    serde_json::from_slice(&bytes)
        .map_err(|e| format!("could not parse {} as a Character: {}", path.display(), e))
}

fn run_render(file: PathBuf, output: Option<PathBuf>) -> ExitCode {
    let character = match load_character(&file) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}", e);
            return ExitCode::from(2);
        }
    };
    let md = character_to_markdown(&character);
    match output {
        Some(path) => {
            if let Err(e) = fs::write(&path, md.as_bytes()) {
                eprintln!("could not write {}: {}", path.display(), e);
                return ExitCode::from(2);
            }
        }
        None => {
            print!("{}", md);
        }
    }
    ExitCode::SUCCESS
}

fn run_validate(file: PathBuf) -> ExitCode {
    let character = match load_character(&file) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}", e);
            return ExitCode::from(2);
        }
    };

    let mut report = ValidationReport::new();
    report.extend(character.validate_chargen());
    report.extend(character.validate_xp());

    print_report(&report);

    if report.is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn print_report(report: &ValidationReport) {
    println!("errors ({}):", report.errors.len());
    for err in &report.errors {
        println!("  - {}", format_err(err));
    }
    println!();
    println!("notes ({}):", report.notes.len());
    for note in &report.notes {
        println!("  - {}", format_err(note));
    }
    println!();
    if report.is_ok() {
        println!("ok: 0 errors");
    } else {
        println!("FAIL: {} error(s)", report.errors.len());
    }
}

fn format_err(err: &ValidationError) -> String {
    // ValidationError implements Display via thiserror.
    err.to_string()
}
