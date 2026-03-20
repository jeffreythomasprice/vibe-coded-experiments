# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

file-sync is a Rust CLI tool that synchronizes files/directories across multiple locations. Use cases include backing up dotfiles to cloud storage or keeping config files in sync across git repos.

## Build & Test Commands

- **Build:** `cargo build`
- **Run:** `cargo run -- [args]`
- **Test all:** `cargo test`
- **Test single:** `cargo test test_name` or `cargo test module::test_name`
- **Check (no codegen):** `cargo check`
- **Clippy:** `cargo clippy`

Rust edition 2024. No async runtime — all I/O is synchronous. Tests exist in `src/tui/diff_model.rs` and `src/resolve.rs`.

## CLI Usage

```
file-sync [--config <path>] [--log-dir <path>] [--backup-dir <path>] [--entity <name>...] [--dry-run] [--verbose]
```

Config resolution: `--config` flag > `$FILE_SYNC_CONFIG` env var > `~/.config/file-sync/config.toml`

### Config file format

```toml
log_dir = "/optional/log/path"      # default: /tmp/file-sync/logs
backup_dir = "/optional/backup/path" # default: /tmp/file-sync/backups

[[sync]]
name = "bashrc"
paths = [
    "~/dotfiles/.bashrc",
    "~/.bashrc",
]
```

Paths support tilde expansion and globs. Directory entities recursively sync all contents.

Optional `include` field filters directory entities to only matching relative paths:
```toml
[[sync]]
name = "shell-config"
paths = ["~/dotfiles", "~/cloud-backup/dotfiles"]
include = ["*.bashrc", "*.zshrc", ".config/starship.toml"]
```
`include` uses globset patterns (`*` matches across `/`, so `*.sh` matches nested files too). Ignored for file-mode entities. Empty/absent means no filtering.

## Architecture

The data flow is: **config loading → path resolution → sync orchestration → (diff dialog if conflict) → file copy**.

- `main.rs` — CLI parsing (clap derive), iterates sync entities, orchestrates the pipeline
- `config.rs` — TOML config structs (`Config`, `SyncEntity`), tilde expansion, config file discovery
- `resolve.rs` — Expands globs, classifies paths (file/dir/missing), validates no mixed types, builds `FileGroup`s. For directory entities, recursively walks all dirs and creates a `FileGroup` per relative path (union of all directories)
- `sync.rs` — Per-`FileGroup` logic: 0 exist → skip, 1 exists → copy to missing, 2+ identical → copy to missing, 2+ differ → launch TUI diff dialog
- `diff.rs` — Binary detection (null bytes in first 8KB), `similar::TextDiff` for text, `ConflictInfo`/`FileCopy` structs
- `copy.rs` — File copy with parent dir creation, dry-run support, automatic backup of overwritten files
- `summary.rs` — Accumulates copies, conflicts, errors, skips; prints end-of-run report
- `logging.rs` — Tracing setup with daily rolling log files + stderr

### TUI Module (`src/tui/`)

Side-by-side diff viewer using ratatui + crossterm. Entry point: `run_diff_dialog(ConflictInfo) -> Result<UserChoice>`.

- `mod.rs` — Terminal setup/teardown (raw mode, alternate screen), event loop
- `app.rs` — `App` state struct, key handling (vim-style + arrow keys), scroll, `UserChoice` enum (Keep/Skip/Quit)
- `diff_model.rs` — `build_side_by_side()` transforms `similar` output into paired `DiffRow` vec for rendering. Has unit tests.
- `widgets.rs` — Renders a single diff pane as styled `Paragraph`
- `layout.rs` — Frame composition: header, two 50/50 split panes, footer keybinding legend
- `styles.rs` — Color constants (red=delete, green=insert, yellow=modified, dim=line numbers)

## Key Design Decisions

- `anyhow::Result` with `.context()` used throughout for error handling
- Per-file errors are logged and recorded in summary but don't halt execution; config errors and mixed-type entities fail fast
- "User quit" propagated as an `anyhow` error string from sync back to main
- Files are compared by full byte content (not mtime or hash)
- Before overwriting any file, a backup is saved to `<backup_dir>/<timestamp>/<entity>/<abs_path>`. Backup dir defaults to `/tmp/file-sync/backups`
- Tilde expansion happens at resolution time, not parse time
