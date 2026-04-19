# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

# llm-rag

Rust CLI + daemon for a local LLM-with-RAG system. Early scaffolding: the client/server transport layer works (ping round-trip), but RAG ingest, vector search, LLM calls, and the ratatui TUI described in `plan-tui-documents.md` are not yet implemented. See `TODO.md` for the feature wishlist.

## Commands

- Build: `cargo build`
- Run a subcommand: `cargo run -- <subcommand>` (e.g. `cargo run -- ping`, `cargo run -- server`)
- Test: `cargo test` / single test: `cargo test <name>`
- Lint: `cargo clippy --all-targets -- -D warnings`
- Format: `cargo fmt`
- Rust edition is `2024` — some language features (e.g. `if let` chains, `let else`) are used; keep MSRV in mind if bumping deps.

## Architecture

### Client/server split with auto-spawn
A single binary runs in two modes:
- `llm-rag server` — long-lived daemon listening on a Unix socket; self-terminates after `server_idle_timeout_secs` of zero active connections (`src/server.rs`).
- Any other subcommand — CLI client that connects to that socket, running a request via `client::round_trip`.

If the socket is missing or connect fails with `NotFound` / `ConnectionRefused`, the client **forks a detached server process** via `spawn_detached_server` (sets up a new session with `setsid`, stdio → `/dev/null`) and then polls the socket for up to ~2s before giving up (`src/client.rs`). Race-safe: if two clients spawn at once, the loser hits `AddrInUse`, confirms the socket is live by connecting, and exits with `ServerStartError::AlreadyRunning`. Stale sockets (bind EADDRINUSE but no one listening) are cleaned up before re-bind.

### Wire protocol
Length-delimited framing over the Unix stream using `tokio_util::codec::LengthDelimitedCodec` (u32 big-endian prefix, 16 MiB cap). Payload is JSON-serialized `Request` / `Response` enums tagged with `#[serde(tag = "type")]` (`src/protocol.rs`). To add a new RPC: extend both enums, then add a match arm in `handlers::dispatch` and a branch in `main::run` that constructs the request and interprets the response.

### Socket + config paths (`src/paths.rs`)
- Socket: XDG `runtime_dir` if available (e.g. `/run/user/<uid>/llm-rag.sock`), otherwise `/tmp/llm-rag-$USER.sock`.
- Config search order: `./config.toml`, then `$XDG_CONFIG_HOME/llm-rag/config.toml`. A `--config <path>` override is a hard-fail if the file is missing (`ConfigError::NotFound`); if no override is given and *no* default-search file exists, that is also `ConfigError::NotFound`. The loader does not fall back to a built-in default — `Config` has no `Default` impl.
- Secrets search order: `./secrets.toml`, then `$XDG_CONFIG_HOME/llm-rag/secrets.toml`. A `--secrets <path>` override is a hard-fail (`ConfigError::SecretsNotFound`) if missing, but missing default-search files **silently** yield `SecretsConfig::default()` (all `None`). Features that need a secret error at use-site, not at load. Secret values are `secrecy::SecretString` — `Debug` prints `[REDACTED]` and the value is zeroized on drop. Loader sets `SecretsConfig::loaded_from_insecure_path` when the file is group/world-readable on Unix; `main` emits a deferred `tracing::warn!` after `logging::init`.

### Error taxonomy and exit codes (`src/error.rs`)
Errors are split by layer: `ConfigError`, `ServerStartError`, `ClientError`, `ProtocolError`, all funneled into `CliError` which `main` renders as a one-line JSON blob on stderr (`{"error": "<Variant>", ...}`) plus a specific exit code:

| Code | Meaning |
|------|---------|
| 10 | `ServerStart::AlreadyRunning` |
| 11 | `Client::ConnectTimeout` |
| 12 | `Client::RequestTimeout` |
| 13 | `Config::NotFound` or `Config::SecretsNotFound` |
| 1  | everything else |

Callers (other CLIs, scripts) are expected to parse the JSON line; keep this contract stable when adding variants.

## Error handling conventions

- Prefer creating a new error type whenever we know exactly the kinds of errors that can occur. Each layer has its own `thiserror` enum (see `src/error.rs`); new modules should follow the same pattern and be added as a `#[from]`/`#[source]` variant on `CliError` if they surface to main.
- If we don't know the full set of errors, use `anyhow` — `CliError::Other(#[from] anyhow::Error)` is the escape hatch.
- Machine-readable stderr output: when adding a new error variant that should be distinguishable by scripts, extend `CliError::to_json_line` with a dedicated JSON shape rather than falling through to the generic branch.

## Planning docs

- `TODO.md` — feature wishlist (hybrid search, small-to-large chunking, OCR, TUI, etc.) — scope is still fluid.
- `plan-tui-documents.md` — locked UX spec for the document-management TUI (screens, keybindings, autocomplete semantics). When implementing the TUI, treat this as the source of truth for behavior; file layout suggestions at the bottom (§10) are advisory.
