//! `clap` types for the `ecs` binary. Lives in the library so
//! integration tests can exercise `Cli::try_parse_from` without spawning the
//! binary.

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(
    name = "ecs",
    version,
    about = "ecs (Exalted Character Sheet) — Exalted 2e character sheet tools. \
             With no subcommand, launches the GUI editor; an optional FILE \
             positional opens that character on startup."
)]
pub struct Cli {
    /// Output format for command results and error messages.
    ///
    /// `text` is the default human-readable form. `json` emits machine-parsable
    /// output for `validate` and JSON-encoded `{"error": "..."}` on stderr for
    /// any command that fails. The intended payload of `render` (markdown) is
    /// unaffected; only its error messages honor this flag. Ignored when no
    /// subcommand is given (the GUI has no text/json mode).
    #[arg(
        long = "output-format",
        value_enum,
        global = true,
        default_value_t = OutputFormat::Text,
    )]
    pub output_format: OutputFormat,

    /// Character file to open in the GUI editor. Only used when no
    /// subcommand is given.
    #[arg(value_name = "FILE", index = 1)]
    pub file: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Cmd>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum RenderFormat {
    Markdown,
    Pdf,
}

/// Which embedded rules-summary markdown file to emit.
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum RulesTopic {
    /// `rules/game_rules.md` — core-rules summary.
    Rules,
    /// `rules/character_creation.md` — chargen summary.
    Chargen,
}

#[derive(Subcommand)]
pub enum Cmd {
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
