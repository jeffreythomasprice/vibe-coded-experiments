use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use serde::Serialize;

use exalted::error::{ValidationError, ValidationReport};
use exalted::render::{
    background_to_markdown, backgrounds_to_markdown, character_to_markdown, character_to_pdf,
    charm_to_markdown, charms_to_markdown, spell_to_markdown, spells_to_markdown,
};
use exalted::rules::database::{
    character_creation_markdown, database, game_rules_markdown, init_database, BackgroundEntry,
    CharmEntry, SpellEntry,
};
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

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum RenderFormat {
    Markdown,
    Pdf,
}

/// Which embedded rules-summary markdown file to emit.
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum RulesTopic {
    /// `rules/game_rules.md` — core-rules summary.
    Rules,
    /// `rules/character_creation.md` — chargen summary.
    Chargen,
}

#[derive(Subcommand)]
enum Cmd {
    /// Render a character TOML file as markdown or a filled PDF.
    ///
    /// Markdown (default) writes to stdout unless `-o` is given.
    /// PDF requires `-o FILE` because PDF bytes are binary and not safe to
    /// write to a TTY.
    Render {
        /// Path to the character TOML file.
        file: PathBuf,
        /// Optional output path. Required when `--format pdf` is selected;
        /// for markdown, omitting it writes to stdout.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Rendered output format.
        #[arg(long = "format", value_enum, default_value_t = RenderFormat::Markdown)]
        format: RenderFormat,
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
    /// Emit one of the embedded rules-summary markdown documents.
    ///
    /// The payload is always markdown written to stdout — `--output-format`
    /// is ignored for this subcommand (a JSON wrapper around a multi-page
    /// document is not useful).
    RulesMarkdown {
        /// Which document to emit.
        topic: RulesTopic,
    },
    /// List all backgrounds, or show one by id.
    ///
    /// With no id, every background is emitted (sorted by id). With an id,
    /// only that entry is emitted; an unknown id exits with status 2.
    /// `--output-format text` (default) emits markdown; `--output-format
    /// json` emits the entry struct(s).
    Backgrounds {
        /// Optional background id (e.g. `allies`, `artifact`).
        id: Option<String>,
    },
    /// List all charms, or show one by id.
    ///
    /// Excellency template ids (e.g. `first-ability-excellency`) are
    /// expanded at startup into one entry per Ability; pass the expanded id
    /// (e.g. `first-archery-excellency`).
    Charms {
        /// Optional charm id.
        id: Option<String>,
    },
    /// List all spells, or show one by id.
    Spells {
        /// Optional spell id.
        id: Option<String>,
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
        Cmd::Render { file, output, format } => run_render(file, output, format, fmt),
        Cmd::Validate { file } => run_validate(file, fmt),
        Cmd::RulesMarkdown { topic } => run_rules_markdown(topic),
        Cmd::Backgrounds { id } => run_backgrounds(id, fmt),
        Cmd::Charms { id } => run_charms(id, fmt),
        Cmd::Spells { id } => run_spells(id, fmt),
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

fn run_render(
    file: PathBuf,
    output: Option<PathBuf>,
    format: RenderFormat,
    fmt: OutputFormat,
) -> ExitCode {
    let character = match load_character(&file) {
        Ok(c) => c,
        Err(e) => {
            emit_error(&e, fmt);
            return ExitCode::from(2);
        }
    };
    match format {
        RenderFormat::Markdown => {
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
        }
        RenderFormat::Pdf => {
            let path = match output {
                Some(p) => p,
                None => {
                    emit_error(
                        "PDF output requires -o FILE (refusing to write binary to stdout)",
                        fmt,
                    );
                    return ExitCode::from(2);
                }
            };
            let bytes = match character_to_pdf(&character) {
                Ok(b) => b,
                Err(e) => {
                    emit_error(&format!("pdf render failed: {}", e), fmt);
                    return ExitCode::from(2);
                }
            };
            if let Err(e) = fs::write(&path, &bytes) {
                emit_error(
                    &format!("could not write {}: {}", path.display(), e),
                    fmt,
                );
                return ExitCode::from(2);
            }
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

// --------------------------------------------------------------------------
// Rules-data subcommands
// --------------------------------------------------------------------------

fn run_rules_markdown(topic: RulesTopic) -> ExitCode {
    let md = match topic {
        RulesTopic::Rules => game_rules_markdown(),
        RulesTopic::Chargen => character_creation_markdown(),
    };
    print!("{}", md);
    if !md.ends_with('\n') {
        println!();
    }
    ExitCode::SUCCESS
}

fn run_backgrounds(id: Option<String>, fmt: OutputFormat) -> ExitCode {
    let db = database();
    match id {
        Some(id) => match db.background(&id) {
            Some(entry) => emit_single(entry, |b| background_to_markdown(b), fmt),
            None => not_found("background", &id, fmt),
        },
        None => {
            let mut entries: Vec<&BackgroundEntry> = db.iter_backgrounds().collect();
            entries.sort_by(|a, b| a.id.cmp(&b.id));
            emit_list(&entries, |e| backgrounds_to_markdown(e), fmt)
        }
    }
}

fn run_charms(id: Option<String>, fmt: OutputFormat) -> ExitCode {
    let db = database();
    match id {
        Some(id) => match db.charm(&id) {
            Some(entry) => emit_single(entry, |c| charm_to_markdown(c), fmt),
            None => not_found("charm", &id, fmt),
        },
        None => {
            let mut entries: Vec<&CharmEntry> = db.iter_charms().collect();
            entries.sort_by(|a, b| a.id.cmp(&b.id));
            emit_list(&entries, |e| charms_to_markdown(e), fmt)
        }
    }
}

fn run_spells(id: Option<String>, fmt: OutputFormat) -> ExitCode {
    let db = database();
    match id {
        Some(id) => match db.spell(&id) {
            Some(entry) => emit_single(entry, |s| spell_to_markdown(s), fmt),
            None => not_found("spell", &id, fmt),
        },
        None => {
            let mut entries: Vec<&SpellEntry> = db.iter_spells().collect();
            entries.sort_by(|a, b| a.id.cmp(&b.id));
            emit_list(&entries, |e| spells_to_markdown(e), fmt)
        }
    }
}

fn emit_single<T, F>(entry: &T, render_md: F, fmt: OutputFormat) -> ExitCode
where
    T: serde::Serialize,
    F: FnOnce(&T) -> String,
{
    match fmt {
        OutputFormat::Text => {
            print!("{}", render_md(entry));
            ExitCode::SUCCESS
        }
        OutputFormat::Json => match serde_json::to_string_pretty(entry) {
            Ok(s) => {
                println!("{}", s);
                ExitCode::SUCCESS
            }
            Err(e) => {
                emit_error(&format!("could not serialize entry: {}", e), fmt);
                ExitCode::from(2)
            }
        },
    }
}

fn emit_list<T, F>(entries: &[&T], render_md: F, fmt: OutputFormat) -> ExitCode
where
    T: serde::Serialize,
    F: FnOnce(&[&T]) -> String,
{
    match fmt {
        OutputFormat::Text => {
            print!("{}", render_md(entries));
            ExitCode::SUCCESS
        }
        OutputFormat::Json => match serde_json::to_string_pretty(entries) {
            Ok(s) => {
                println!("{}", s);
                ExitCode::SUCCESS
            }
            Err(e) => {
                emit_error(&format!("could not serialize entries: {}", e), fmt);
                ExitCode::from(2)
            }
        },
    }
}

fn not_found(kind: &str, id: &str, fmt: OutputFormat) -> ExitCode {
    emit_error(&format!("no such {}: {}", kind, id), fmt);
    ExitCode::from(2)
}
