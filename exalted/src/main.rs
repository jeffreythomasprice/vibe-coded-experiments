use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use serde::Serialize;

use exalted::error::{ValidationError, ValidationReport};
use exalted::render::character_to_markdown;
use exalted::rules::database::init_database;
use exalted::Character;

#[derive(Parser)]
#[command(name = "exalted", version, about = "Exalted 2e character sheet tools")]
struct Cli {
    /// Output format for command results and error messages.
    ///
    /// `text` is the default human-readable form. `json` emits machine-parsable
    /// output for `validate` and JSON-encoded `{"error": "..."}` on stderr for
    /// any command that fails. The intended payload of `render` (markdown) is
    /// unaffected; only its error messages honor this flag.
    #[arg(
        long = "output-format",
        value_enum,
        global = true,
        default_value_t = OutputFormat::Text,
    )]
    output_format: OutputFormat,

    #[command(subcommand)]
    command: Cmd,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
}

#[derive(Subcommand)]
enum Cmd {
    /// Render a character TOML file as markdown to stdout (or `-o FILE`).
    Render {
        /// Path to the character TOML file.
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
        /// Path to the character TOML file.
        file: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let fmt = cli.output_format;
    if let Err(e) = init_database() {
        emit_error(&format!("failed to load rules database: {}", e), fmt);
        return ExitCode::from(2);
    }
    match cli.command {
        Cmd::Render { file, output } => run_render(file, output, fmt),
        Cmd::Validate { file } => run_validate(file, fmt),
    }
}

fn emit_error(msg: &str, fmt: OutputFormat) {
    match fmt {
        OutputFormat::Text => eprintln!("{}", msg),
        OutputFormat::Json => eprintln!("{}", serde_json::json!({ "error": msg })),
    }
}

fn load_character(path: &PathBuf) -> Result<Character, String> {
    let text = fs::read_to_string(path)
        .map_err(|e| format!("could not read {}: {}", path.display(), e))?;
    toml::from_str(&text)
        .map_err(|e| format!("could not parse {} as a Character: {}", path.display(), e))
}

fn run_render(file: PathBuf, output: Option<PathBuf>, fmt: OutputFormat) -> ExitCode {
    let character = match load_character(&file) {
        Ok(c) => c,
        Err(e) => {
            emit_error(&e, fmt);
            return ExitCode::from(2);
        }
    };
    let md = character_to_markdown(&character);
    match output {
        Some(path) => {
            if let Err(e) = fs::write(&path, md.as_bytes()) {
                emit_error(
                    &format!("could not write {}: {}", path.display(), e),
                    fmt,
                );
                return ExitCode::from(2);
            }
        }
        None => {
            print!("{}", md);
        }
    }
    ExitCode::SUCCESS
}

fn run_validate(file: PathBuf, fmt: OutputFormat) -> ExitCode {
    let character = match load_character(&file) {
        Ok(c) => c,
        Err(e) => {
            emit_error(&e, fmt);
            return ExitCode::from(2);
        }
    };

    let mut report = ValidationReport::new();
    report.extend(character.validate_chargen());
    report.extend(character.validate_xp());

    match fmt {
        OutputFormat::Text => print_report(&report),
        OutputFormat::Json => print_report_json(&report),
    }

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

#[derive(Serialize)]
struct JsonReport<'a> {
    ok: bool,
    errors: &'a [ValidationError],
    notes: &'a [ValidationError],
}

fn print_report_json(report: &ValidationReport) {
    let payload = JsonReport {
        ok: report.is_ok(),
        errors: &report.errors,
        notes: &report.notes,
    };
    // Pretty-print for readability; machine parsers handle either.
    match serde_json::to_string_pretty(&payload) {
        Ok(s) => println!("{}", s),
        Err(e) => eprintln!("{}", serde_json::json!({ "error": format!("could not serialize report: {}", e) })),
    }
}

fn format_err(err: &ValidationError) -> String {
    // ValidationError implements Display via thiserror.
    err.to_string()
}
