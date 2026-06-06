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
    Fields: `log_file`, `lock_file`, and `database_file` (engine DB, defaults to
    `~/.config/munchkin/engine.db`).
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
  `db/` owns the database: a local `turso` (SQLite-compatible) file at the
  config's `database_file`, opened via a current-thread tokio runtime that the
  `Db` handle holds for the process lifetime (the rest of the engine stays
  sync — DB work goes through `rt.block_on`). `db/migrations.rs` is a
  hand-rolled, forward-only migration runner: embedded `db/migrations/*.sql`
  files with increasing versions, applied at startup and tracked in a
  `_migrations` table (idempotent). Nothing is persisted yet — `0001_initial.sql`
  is a placeholder.
- **`tui/`** — terminal UI (stub). No lock, no stderr logging. A UI framework
  (ratatui/crossterm) will be added later.

## Startup order (both binaries)

`Cli::parse` → `Config::load` → `logging::init` → log config source + contents →
(engine only) `lock::acquire` → (engine only) `Db::open` (creates the db's parent
dir + runs migrations) → run. The `LogGuard` from `init` must be held for the
whole of `main` — dropping it early stops pid/mode tagging and may drop buffered
log lines; likewise the `Db` handle must outlive `main`. Note the engine now
auto-creates `~/.config/munchkin/` on first run (for the default db path);
previously nothing created that directory.

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
