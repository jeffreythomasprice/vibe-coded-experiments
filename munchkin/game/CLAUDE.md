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
    Fields: `log_file`, `lock_file`, `database_file` (engine DB, defaults to
    `~/.config/munchkin/engine.db`), `socket_file` (engine IPC socket,
    defaults to `/tmp/munchkin-engine.sock`), and an `[ollama]` table for the AI
    backend (`host`=`localhost`, `port`=`11434`, and **per-role** model ids
    `player_model`/`referee_model`, both defaulting to `qwen3.5`; a missing table
    uses all defaults). Only the engine consumes `[ollama]`, but it lives here
    because config parsing is centralised in `shared`.
  - `logging.rs` — `tracing` setup via `init(&Config, AppMode)`. One shared
    append-only log file (so multiple processes can write at once); every line
    is tagged `app{pid=… mode=…}` via a process-lifetime root span held in the
    returned `LogGuard`. Default filter `warn,shared=trace,engine=trace,tui=trace`,
    overridable with `RUST_LOG`. Engine mirrors to stderr; tui does not.
  - `model.rs` — **stub** game types (`GameState`, `Player`, `Card`, …). Will
    eventually mirror the curated schema in `assets/processed/README.md`.
  - `protocol.rs` — the JSON request/response messages the engine and its
    clients exchange (`Request`/`Response`/`ClientInfo`, `PROTOCOL_VERSION`),
    plus newline-delimited-JSON framing (`read_message`/`write_message`).
  - `client.rs` — `IpcClient`, a reusable tokio client (`connect`/`send`/`recv`/
    `request`). One in-flight request at a time — a single stream correlates
    replies only by order.
- **`engine/`** — the game-rules engine. `rules.rs` has stub submodules mirroring
  `assets/processed/rules.md` (turn/combat/items/curses/level/death/win);
  `rules::run` also builds the (stub) referee agent from the `[ollama]` config.
  `ai/` is the AI scaffolding (engine-only, so `reqwest` stays out of `tui`).
  `ai/client.rs` is a **real** async Ollama client (`OllamaClient`, one per role,
  `POST /api/generate`). The rest are **stubs** for the two kinds of LLM
  consultation, split by *information*: **player agents** (`ai/player.rs`) decide
  a seat's moves from a redacted `PlayerView` (own hand + others' *public* state
  incl. hand *sizes*) — invoked for both mandatory decisions (kicked-open monster:
  fight/run/play/concede) and out-of-turn opportunities (help/hinder/pass); the
  **referee** / rules-lawyer (`ai/referee.rs`) rules legality of a `ProposedAction`
  with the *full* `GameState` (all hands). `ai/view.rs::player_view` is the
  redaction that enforces the player-side information boundary; `ai/decision.rs`
  holds the request/decision vocabulary. The agent stubs build placeholder prompts,
  call Ollama best-effort, log the raw reply, and fall back to safe defaults
  (player → pass/concede, referee → "legal"), so the engine builds and runs with
  or without a reachable Ollama.
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
  `server.rs` + `registry.rs` are the IPC server: it binds the config's
  `socket_file` (removing a stale leftover first — safe because the lock is
  held), serves one tokio task per connection, tracks connected clients in a
  `std::sync::Mutex`-guarded `Registry`, and a reaper task kicks (actively closes)
  any client idle > 2 min. Protocol stub: `Hello`/`Ping`/`Stats`. Bind failure →
  log + exit non-zero.
- **`tui/`** — terminal UI (ratatui + crossterm). No lock, no stderr logging
  (it owns the terminal, so logs go to the shared file only — a stray stderr
  write would corrupt the UI). `session.rs` connects + does the `Hello`
  handshake (a missing engine still fails fast, non-zero — this happens *before*
  the terminal is taken over). `app.rs` then takes over the terminal and runs
  the render loop: a `tokio::select!` over a ~30s keepalive ping, a 1s redraw
  tick, and crossterm's async key `EventStream`; **Esc quits.** `ui.rs` paints a
  placeholder status dashboard (connection state, server identity, time since
  last ping, the connected-client list). A mid-session drop/kick is *not* fatal:
  it flips the view to "disconnected" and stops pinging, but stays up until Esc.
  The terminal is restored on every exit path via a `Drop` guard in `main.rs`.

## Startup order (both binaries)

`Cli::parse` → `Config::load` → `logging::init` → log config source + contents →
**engine:** `lock::acquire` → `Db::open` (creates the db's parent dir + runs
migrations) → `server::run` (bind socket, serve until killed); **tui:**
`session::connect` (connect + handshake, fail-fast non-zero if the engine isn't
running) → take over the terminal → `app::run_ui` (render until Esc).
The `LogGuard` from `init` must be held for the whole of `main` — dropping it
early stops pid/mode tagging and may drop buffered log lines, so never move it
into a spawned task; likewise the `Db` handle and single-instance lock must
outlive `main`. The engine auto-creates `~/.config/munchkin/` on first run (for
the default db path).

The tui is `#[tokio::main(flavor = "current_thread")]`. The engine keeps a
**synchronous** `main`: its `db` layer owns a current-thread runtime and uses
`block_on`, which would panic nested inside `#[tokio::main]`, so the engine
builds its own multi-thread runtime and `block_on`s `server::run`.

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

- Change the config schema (incl. the `[ollama]` table) → update `config.rs`,
  `config.example.toml`, and this file.
- Change the logging scheme → update `logging.rs` and this file.
- Implement a rules system → fill in the matching `engine/src/rules.rs`
  submodule; keep it traceable back to `assets/processed/rules.md`.
