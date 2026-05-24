# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

# Purpose

This is a project about helping create and manage character sheets for Exalted (2nd edition). It is a Rust crate that exposes both a library (`src/lib.rs`) and a CLI binary (`src/main.rs`, name `exalted`).

# Common commands

```bash
# Build
cargo build                 # debug
cargo build --release       # release; the create-character skill expects target/release/exalted

# Run without installing.
# With NO subcommand, the binary launches the egui-based GUI editor (see
# `src/ui/`). An optional positional FILE opens that character on startup.
cargo run                                # blank GUI
cargo run -- assets/sample-character.toml  # GUI, file pre-opened

# CLI subcommands
cargo run -- render assets/sample-character.toml
cargo run -- render --format pdf assets/sample-character.toml -o /tmp/sheet.pdf
cargo run -- validate assets/sample-character.toml
cargo run -- --output-format json validate assets/sample-character.toml

# Subcommands the CLI exposes:
#   render, validate, rules-markdown {rules|chargen}, backgrounds [id], charms [id], spells [id]
# All accept the global flag `--output-format {text|json}` (text is default; for
# list/show commands text is markdown, json is the raw struct). The flag is
# ignored when no subcommand is given — the GUI has no text/json mode.

# Tests
cargo test                          # whole suite (integration tests in tests/ + unit tests)
cargo test --test xp_accounting     # one integration test file
cargo test <name_substring>         # filter by test name

# Lint
cargo clippy --all-targets
cargo fmt

# Examples (one-off operator helpers, not part of the library API)
cargo run --example dump_dots       # dumps PDF dot-field coordinates; used when re-deriving field_map.rs
```

`cargo test` can take a while; the PDF-render integration test (`tests/pdf_render.rs`) loads and re-serializes the embedded AcroForm template, which is the slow part.

# Architecture

## Top-level data flow

TOML on disk → `Character` struct → validation report → renderer.

- **`src/character/`** — the `Character` struct and its constituent types (`Identity`, `RatedTrait`, `CharmRef`, `BackgroundRef`, `SpellRef`, `Equipment`, `PoolState`, `Intimacy`, `Combo`, …). This is the serialization surface: the TOML files under `assets/` and any user character file deserialize directly into `Character`. Every dot purchase carries a `DotSource` discriminator (`Base | ChargenPriority | BonusPoints { spent } | Xp { spent }`) so validation can reconstruct what was paid for what.
- **`src/rules/`** — pure rules logic that operates on a `&Character`. Modules: `chargen` (BP/XP ledgers and the chargen validation entry points), `xp_costs`, `dice`, `defense`, `health`, `derived`, `anima`, `essence`, `equipment`, `backgrounds`, `languages`, and `database` (see below). `validate_chargen` and `validate_xp` are the two functions invoked by the `validate` subcommand.
- **`src/render/`** — output formats. `markdown.rs` produces the human-readable sheet; `pdf/` fills the embedded MrGone AcroForm template (`assets/character-sheet/Exalted2ndED4-Page_TheSolarsV2_Editable.pdf`); `rules_data.rs` produces the markdown for the `backgrounds` / `charms` / `spells` CLI commands.
- **`src/ui/`** — the egui/eframe desktop editor (entry point `ui::launch`). It edits the same `Character` struct used everywhere else: `ui::io` round-trips TOML through `toml::{from_str, to_string_pretty}`, and `AppState` holds the live `Character`, dirty flag, file path, the cached `ValidationReport`, and per-picker state. Submodules: `app` (the `eframe::App` impl and per-frame draw loop), `state` (`AppState`, `StartupAction`, `PendingAction`, status toasts), `menu` (File menu / shortcuts), `sidebar` (derived stats + validation dock), `sections/` (one file per character-sheet section: attributes, abilities, charms, spells, backgrounds, intimacies, combos, pools, xp, …), `pickers/` (modal browsers backed by the rules database for adding charms/spells/backgrounds), `dialogs/` (confirm-discard, file pickers via `rfd`), and `widgets/` (shared building blocks like `RatedTrait` editors and `DotSource` selectors).
- **`src/error.rs`** — `ValidationError` (typed errors, `thiserror`-derived) and `ValidationReport { errors, notes }`. Validators across `rules/` push into a `ValidationReport`; the CLI prints it as text or JSON, and the GUI displays it in the sidebar validation dock.

## Embedded rules database

`src/rules/database/` parses three TOML files and two markdown files at startup via `include_str!`:

- `rules/charms.toml` → `CharmEntry` map
- `rules/spells.toml` → `SpellEntry` map
- `rules/backgrounds.toml` → `BackgroundEntry` map
- `rules/game_rules.md` and `rules/character_creation.md` → emitted by `rules-markdown`

`init_database()` runs once at `main()` startup and stores a `RulesDatabase` in a `OnceLock`; `database()` returns it. Tests that need it must call `init_database()` themselves (see `tests/common/mod.rs`).

The five universal Excellency template ids (`first-ability-excellency`, `second-ability-excellency`, `third-ability-excellency`, `infinite-ability-mastery`, `ability-essence-flow`) are **expanded at load time** into 25 derived entries each — one per `AbilityKind`. Characters reference the expanded id, e.g. `first-archery-excellency`. Don't look for a generic excellency entry; it doesn't exist post-expansion.

Because the rules files are embedded at compile time, editing any file under `rules/` requires a rebuild before the change is visible to the CLI.

## PDF rendering

`src/render/pdf/mod.rs::character_to_pdf` is the entry point. The pipeline:

1. `template::load_template` parses the embedded editable PDF.
2. `acroform::FieldIndex::build` indexes every AcroForm widget by `/T` name.
3. `text_fields::fill`, `checkboxes::fill`, `dots::fill` write values via the index.
4. `acroform::finalize_form` finalizes the form so viewers display the filled values.

Dot positions are hard-coded in `pdf/field_map.rs` (~700 lines of coordinates) because the PDF template's dot widgets are named `dot12`, `dot13`, … with no semantic linkage to the trait they belong to. `examples/dump_dots.rs` is the operator tool used to re-derive these mappings when the template changes — it walks the AcroForm and clusters widgets by page+row.

## GUI editor

`src/ui/launch` opens a native window via `eframe::run_native` (egui 0.34). The editor is an immediate-mode UI over a single in-memory `Character` held in `AppState`. Each frame, `app::App::update` lays out the menu, sidebar (derived stats + validation dock), and the central scrolling region of sheet sections. Section render functions take `&mut AppState` and call `mark_dirty` whenever they mutate the character so the title bar's `*` indicator, the save-prompt flow, and the validation re-run all stay in sync. Validation is recomputed lazily when `validation_dirty` is set, not on every frame.

Save / load goes through `ui::io` (TOML round-trip on the same `Character` the CLI uses), so a character authored in the GUI is byte-for-byte the same shape as one authored by the `create-character` skill or by hand. File-system dialogs use `rfd`; the rules database (charms, spells, backgrounds) is read by the picker modals to populate their lists.

# Game rules / references

Two summary documents live in `rules/`:

- `rules/game_rules.md` — summary of the core game rules.
- `rules/character_creation.md` — summary of the character creation process.

Treat these as **summaries only**. They are useful for quick orientation and the common cases, but they are not authoritative and may omit detail or nuance.

For anything ambiguous, disputed, or not fully covered by the summaries, look up the actual rule books using the `document-search` skill. Always pass `--tags exalted` so the search is scoped to the right books (as opposed to unrelated documents). Prefer the rule books over the summaries whenever the two disagree, and consider updating the relevant summary once the rule book answer is known.

# Character creation

For end-to-end character creation, use the `create-character` skill in `.claude/skills/create-character/`. It documents the full TOML schema and the recommended interactive flow, and assumes the release binary at `target/release/exalted` is built.

# TODO.md

Never edit `TODO.md`. It is maintained by the human as they review AI output — do not add, remove, or check off items there. If you want to track work within a session, use your own task list instead.
