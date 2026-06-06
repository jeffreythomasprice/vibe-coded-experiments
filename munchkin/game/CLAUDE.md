# game/ — agent notes

A Cargo **workspace** (edition 2024) for actually *playing* Munchkin. Separate
from the standalone `card-image-scanner/` crate; they share no `Cargo.lock`.

Three crates:

- **`shared/`** — infrastructure + data structures used by every client.
  - `config.rs` — config search + parse. Search order, first existing wins:
    `--config <PATH>` → `./config.toml` → `~/.config/munchkin/config.toml` →
    built-in defaults. `Config::load` returns a `LoadedConfig` carrying the
    `Source` and raw text so the caller can log provenance *after* logging is up
    (logging can't run during load — it needs the log path from the config).
  - `logging.rs` — `tracing` setup via `init(&Config, AppMode)`. One shared
    append-only log file (so multiple processes can write at once); every line
    is tagged `app{pid=… mode=…}` via a process-lifetime root span held in the
    returned `LogGuard`. Default filter `warn,shared=trace,engine=trace,tui=trace`,
    overridable with `RUST_LOG`. Engine mirrors to stderr; tui does not.
  - `model.rs` — **stub** game types (`GameState`, `Player`, `Card`, …). Will
    eventually mirror the curated schema in `assets/processed/README.md`.
- **`engine/`** — the game-rules engine. `rules.rs` has stub submodules mirroring
  `assets/processed/rules.md` (turn/combat/items/curses/level/death/win).
  `lock.rs` is the single-instance lock: an advisory `fs4` exclusive lock on the
  config's `lock_file`, released automatically when the process exits. A second
  engine that starts while one is running aborts immediately.
- **`tui/`** — terminal UI (stub). No lock, no stderr logging. A UI framework
  (ratatui/crossterm) will be added later.

## Startup order (both binaries)

`Cli::parse` → `Config::load` → `logging::init` → log config source + contents →
(engine only) `lock::acquire` → run. The `LogGuard` from `init` must be held for
the whole of `main` — dropping it early stops pid/mode tagging and may drop
buffered log lines.

## Run

```sh
cd game
cargo build
cargo run -p engine                       # logs to file + stderr, takes the lock
cargo run -p tui                          # logs to file only
cargo run -p engine -- --config ./config.example.toml
RUST_LOG=engine=info cargo run -p engine  # override the default filter
```

## Keeping docs in sync

- Change the config schema → update `config.rs`, `config.example.toml`, and this
  file.
- Change the logging scheme → update `logging.rs` and this file.
- Implement a rules system → fill in the matching `engine/src/rules.rs`
  submodule; keep it traceable back to `assets/processed/rules.md`.
