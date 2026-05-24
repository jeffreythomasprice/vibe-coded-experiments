use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use serde::Serialize;

use exalted::cli::{Cli, Cmd, OutputFormat, RenderFormat, RulesTopic};
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

fn main() -> ExitCode {
    let cli = Cli::parse();
    let fmt = cli.output_format;
    if let Err(e) = init_database() {
        emit_error(&format!("failed to load rules database: {}", e), fmt);
        return ExitCode::from(2);
    }
    match cli.command {
        Some(Cmd::Render { file, output, format }) => run_render(file, output, format, fmt),
        Some(Cmd::Validate { file }) => run_validate(file, fmt),
        Some(Cmd::RulesMarkdown { topic }) => run_rules_markdown(topic),
        Some(Cmd::Backgrounds { id }) => run_backgrounds(id, fmt),
        Some(Cmd::Charms { id }) => run_charms(id, fmt),
        Some(Cmd::Spells { id }) => run_spells(id, fmt),
        None => run_ui(cli.file),
    }
}

fn run_ui(file: Option<PathBuf>) -> ExitCode {
    let start = match file {
        Some(p) => exalted::ui::StartupAction::OpenFile(p),
        None => exalted::ui::StartupAction::Empty,
    };
    match exalted::ui::launch(start) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ui error: {}", e);
            ExitCode::from(2)
        }
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
