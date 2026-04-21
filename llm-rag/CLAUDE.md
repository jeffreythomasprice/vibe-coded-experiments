# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

# llm-rag

Rust CLI + daemon for a local LLM-with-RAG system. The client/server transport, streaming chat against Ollama/Anthropic, a Turso-backed store for conversations/messages/tags (and the chunk-table scaffolding for future RAG), and a ratatui TUI with chat/conversation-list/search/tag-editor screens are all implemented. Document ingest + vector search + citation UI are not yet wired up — see `TODO.md` for the wishlist.

## Commands

- Build: `cargo build`
- Run a subcommand: `cargo run -- <subcommand>` (e.g. `cargo run -- ping`, `cargo run -- server`, `cargo run -- conversations list`)
- Run with no subcommand: `cargo run` launches the interactive TUI.
- Test: `cargo test` / single test: `cargo test <name>`
- Test with the in-process mock LLM (skips real Ollama/Anthropic calls): `cargo test --features mock-llm`
- Lint: `cargo clippy --all-targets -- -D warnings`
- Format: `cargo fmt`
- Rust edition is `2024` — `if let` chains and `let else` are used; keep MSRV in mind if bumping deps.

## Architecture

### Client/server split with auto-spawn
A single binary runs in two modes:
- `llm-rag server` — long-lived daemon listening on a Unix socket; self-terminates after `server_idle_timeout_secs` of zero active connections (`src/server.rs`).
- Any other subcommand (including no subcommand → TUI) — CLI client that connects to that socket.

If the socket is missing or connect fails with `NotFound` / `ConnectionRefused`, the client **forks a detached server process** via `spawn_detached_server` (new session with `setsid`, stdio → `/dev/null`) and polls the socket for up to ~2s before giving up (`src/client.rs`). Race-safe: if two clients spawn at once, the loser hits `AddrInUse`, confirms the socket is live by connecting, and exits with `ServerStartError::AlreadyRunning`. Stale sockets (bind EADDRINUSE but no one listening) are cleaned up before re-bind.

### Wire protocol (`src/protocol.rs`)
Length-delimited framing over the Unix stream via `tokio_util::codec::LengthDelimitedCodec` (u32 big-endian prefix, 16 MiB cap). Payload is JSON-serialized `Request` / `Response` enums tagged with `#[serde(tag = "type")]`.

Most RPCs are single-frame request→single-frame response and use `client::round_trip`. **Chat is streaming**: `client::chat_stream` sends one `Request::Chat` and consumes many `Response` frames until a terminal one arrives.

- Start: `Response::ChatStart { conversation_id }` — first frame, mints or confirms the id.
- Deltas: zero-or-more `ChatDelta { text }` (assistant text) and/or `ChatToolCallDelta { id, name, arguments_delta }` (tool-call argument fragments; `name` is only set on the first fragment per `id`).
- Terminator: either `ChatDone { messages_appended }` (success; rows have been persisted to SQLite) or `Response::Error` (failure; nothing partial persisted).

`client_stream_idle_timeout_secs` is a per-frame idle timeout (separate from the single-shot `client_request_timeout_secs`) that turns a silently-hung server into `ClientError::RequestTimeout` instead of an unbounded wait.

**Adding a new RPC**: extend `Request` + `Response` in `src/protocol.rs`, add a match arm in `handlers::dispatch`, and add a branch in `main::dispatch` that builds the request and renders the response.

### LLM layer (`src/llm/`)
- `LlmStack { chat, embeddings }` holds boxed `Arc<dyn LlmProvider>` + `Arc<dyn EmbeddingProvider>`. Providers live in submodules: `ollama`, `anthropic`, and `mock` (feature-gated).
- Chat and embeddings are built separately. `build_chat` is a pure function of config + secrets. `build_embeddings` takes a `dimensions: usize` because the per-dimension chunk table is chosen at startup — dimensions are resolved by the DB layer: cache lookup in `embedding_model_dimensions`, or one probe call against the embedding model. See `server::serve` for the ordering (chat → open DB + resolve dims → embeddings → accept).
- Streaming: `LlmProvider::chat_stream` returns a `Stream<Item = Result<StreamChunk, LlmError>>` emitting `Text`, `ToolCallDelta`, or `Done`. `handlers::handle_chat_inner` is the one consumer; it aggregates chunks into persisted rows.

### Database (`src/db/`)
Turso (libSQL) with a tiny embedded migration runner (`migrations.rs`) that reads numbered `.sql` files via `include_str!` and records applied versions in `_schema_migrations`. `Dal::open` runs pending migrations, resolves the embedding model's vector length (cache or probe), and ensures a per-dimension `document_chunks_<N>` table exists — swapping embedding models with different lengths won't silently corrupt existing rows.

All conversation/message/tag CRUD, message history for a conversation, and the future chunk-search API live on `Dal`. `handlers::dispatch` is the only caller; the DAL is never exposed across the socket.

`MessageRole` includes `ToolUse` + `Tool` rows, and `MessageMetadata` (stored as JSON) carries the tool-call id/name/arguments so a streamed turn containing tool calls can be replayed faithfully on reload (see `handlers::stored_to_llm`).

### TUI (`src/tui/`)
Single `tokio::select!` loop in `tui::run_loop` over three sources: terminal key events, `AppEvent::Reply` from spawned request tasks, and a spinner tick.

- **Screens** (`screens/`): one module per screen, each exporting a `State` struct, a `handle_key` returning a `Transition`, and (usually) a `handle_reply`. The outer loop owns the event dispatch; screens never touch the socket directly.
- **Async requests**: `spawn_request` fires a request on a tokio task, tagging it with a monotonically-increasing `seq`. The screen stores the `seq` in its state (`request_seq`, `tags_seq`, etc.); `handle_reply` ignores replies whose `seq` doesn't match any outstanding request on the current screen — this is how navigating away from a pending request cleanly discards its late reply.
- **Streaming chat in the TUI**: `spawn_request` special-cases `Request::Chat` to use `client::chat_stream`. Each frame arrives as a separate `AppEvent::Reply` with `terminal: false` except the last; `app.pending` is decremented only on the terminal frame so the spinner keeps ticking through the stream.
- **Optimistic UI**: `apply_tag_action` mutates the tag list locally before sending the `ConversationAddTag` / `RemoveTag` RPC — the ack is treated as a no-op in `tag_editor::handle_reply`. On server error the optimistic change would be out of sync; this is a known gap.

### Socket + config paths (`src/paths.rs`)
- Socket: XDG `runtime_dir` if available (e.g. `/run/user/<uid>/llm-rag.sock`), otherwise `/tmp/llm-rag-$USER.sock`.
- Config search order: `./config.toml`, then `$XDG_CONFIG_HOME/llm-rag/config.toml`. A `--config <path>` override is a hard-fail if the file is missing (`ConfigError::NotFound`); if no override is given and *no* default-search file exists, that is also `ConfigError::NotFound`. The loader does not fall back to a built-in default — `Config` has no `Default` impl.
- **Relative `db_path` is resolved against the loaded config file's directory**, not the process CWD — so `db_path = "./local.db"` in `~/.config/llm-rag/config.toml` lives next to that config regardless of where the binary was invoked.
- Secrets search order: `./secrets.toml`, then `$XDG_CONFIG_HOME/llm-rag/secrets.toml`. A `--secrets <path>` override is a hard-fail (`ConfigError::SecretsNotFound`) if missing, but missing default-search files **silently** yield `SecretsConfig::default()` (all `None`). Features that need a secret error at use-site, not at load. Secret values are `secrecy::SecretString` — `Debug` prints `[REDACTED]` and the value is zeroized on drop. Loader sets `SecretsConfig::loaded_from_insecure_path` when the file is group/world-readable on Unix; `main` emits a deferred `tracing::warn!` after `logging::init`.

### Error taxonomy and exit codes (`src/error.rs`)
Errors are split by layer: `ConfigError`, `ServerStartError`, `ClientError`, `ProtocolError`, `LlmError`, `DbError`, `TuiError`, all funneled into `CliError`, which `main` renders as a one-line JSON blob on stderr (`{"error": "<Variant>", ...}`) plus a specific exit code:

| Code | Meaning |
|------|---------|
| 10 | `ServerStart::AlreadyRunning` |
| 11 | `Client::ConnectTimeout` |
| 12 | `Client::RequestTimeout` |
| 13 | `Config::NotFound`, `Config::SecretsNotFound`, or `Llm::MissingSecret` |
| 14 | `Db::Open` or `Db::Migrate` |
| 1  | everything else |

Callers (other CLIs, scripts) are expected to parse the JSON line; keep this contract stable when adding variants.

## Error handling conventions

- Prefer creating a new error type whenever we know exactly the kinds of errors that can occur. Each layer has its own `thiserror` enum (see `src/error.rs` plus the per-module `error.rs` files under `llm/`, `db/`, `tui/`); new modules should follow the same pattern and be added as a `#[from]` variant on `CliError` if they surface to main.
- If we don't know the full set of errors, use `anyhow` — `CliError::Other(#[from] anyhow::Error)` is the escape hatch.
- Machine-readable stderr output: when adding a new error variant that should be distinguishable by scripts, extend `CliError::to_json_line` with a dedicated JSON shape rather than falling through to the generic branch.

## Planning docs

- `TODO.md` — feature wishlist (hybrid search, small-to-large chunking, OCR, document ingest, YouTube transcript ingest, etc.). Scope is still fluid.
