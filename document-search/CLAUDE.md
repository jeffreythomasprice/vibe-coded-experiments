# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build / run / test

```sh
cargo build --release
cargo run -- <subcommand>                    # uses ./config.toml or ~/.config/document-search/config.toml
cargo run -- --config ./other.toml <sub>     # explicit override (errors if missing)
cargo test                                   # all unit tests
cargo test --lib config::                    # one module
cargo test split_sql_handles_basic           # one test by name
RUST_LOG=document_search=debug cargo run     # default is `warn,document_search=trace`
```

Reset state (kill server, drop socket, wipe DB):
```sh
killall -9 document-search 2>/dev/null
rm -f /tmp/document-search.sock
rm -f document-search.db document-search.db-wal document-search.db-shm
```

## Architecture

### Client/server over a Unix socket
The same binary is both client and server. CLI invocations (`ingest`, `info`, `text`, `list`, etc.) run in client mode: they connect to the Unix socket from `[server].socket_path`, and **auto-spawn a detached server (via `setsid` + `Stdio::null()`) if the socket is absent**. The server auto-exits after `idle_timeout_secs` with no clients, no in-flight job, and an empty queue. `cargo run -- server` runs the server in the foreground.

The wire protocol (`src/protocol.rs`) is NDJSON: client sends one `Request` line, server streams `Event` lines until `Event::Final`. Every code path that crosses the socket goes through these enums — when adding a subcommand, plumb a new `Request` variant + `Command` variant + `Event` variant if it needs new progress signaling.

### Single-worker queue with inline bypasses
`src/server/mod.rs::worker_loop` pulls one `Job` at a time off an mpsc channel and runs ingests, info lookups, text reads, etc. serially. **`Status`, `List`, `Cancel`, and `TagList` bypass the queue** — they're handled inline by the per-connection task in `src/server/connection.rs` so they stay responsive while a long ingest is running. The worker's `unreachable!()` branches enforce that. Mutating tag operations (`TagAdd`, `TagRemove`) and `Delete` go through the worker because they write through the shared connection.

`ServerState` (queue + current job + shared `Arc<Db>`) is behind a `std::sync::Mutex`; **only lock for short sync sections, never across `.await`** — the codebase relies on this to keep status snapshots cheap.

Cooperative cancel: each running job gets a `watch::Sender<bool>`. Only `ingest` polls the receiver (between chunks); other handlers ignore it. Hence the `is_ingest` flag on `RunningJob` and the early-out in the `Cancel` handler.

### DB layout: per-dimension chunks table
`src/db/mod.rs::open` resolves the embedding model's vector length once (probing Ollama and caching in `embedding_model_dimensions`), then creates `chunks_<N>` with an `F32_BLOB(N)` column so turso's native `vector_distance_cos` works against it. The `chunks_<N>` table is 1:1 with `document_chunk` keyed by `chunk_id` (cascaded delete).

Switching embedding models with a different vector length means a new `chunks_<N>` table appears; the old one is left in place. Code that reads embeddings should always use `db.chunks_table` — never hard-code a name.

Tags live in `document_tag (document_id, tag)` (migration `0002_tags.sql`) with `ON DELETE CASCADE`, so removing a document drops its tags automatically. Tags are normalized to lowercase + trimmed at the command layer before they hit the DB; queries (e.g. `list --tag` with `--match-all`) assume the stored form is already canonical.

### Single shared `Connection` and the `fresh_conn` escape hatch
The server holds one `turso::Connection` (`db.conn`) and ingests open a transaction on it via raw `BEGIN`/`COMMIT`/`ROLLBACK` strings (turso 0.4 has no async transaction handle). Anything reading concurrently with an in-flight ingest **must** call `Db::fresh_conn()` to get its own connection — `commands::list_text` does this so the inline `list` handler doesn't land inside the worker's open transaction. New read-only inline handlers should follow the same pattern.

### Migrations
`src/db/migrations.rs` is a tiny embedded runner. Each migration is `include_str!`'d from `src/db/migrations/<NNNN>_<name>.sql`. To add one: drop a new file in that directory, append a `(version, name, include_str!(...))` tuple to `MIGRATIONS`, and let monotonic version ordering handle ordering. The runner wraps each file in `BEGIN`/`ROLLBACK` and uses a hand-rolled SQL statement splitter (`split_sql`) — semicolons inside single-quoted strings and `--` line comments are handled, but **multi-statement DDL with `BEGIN…END` blocks or dollar-quoting is not**.

### Canonical PDF text and offset invariant
`src/pdf.rs` joins extracted pages with a U+000C form-feed separator into one canonical `full_text`. **All byte/char offsets in the DB (`document_chunk`, `document_page`, totals) are computed against this exact string** — so chunkers, byte-range readers, and page lookups all share one coordinate system. If you change the separator or the page-join logic, every downstream offset breaks.

`page_for_byte_or_preceding` exists because chunks can start in the separator gap between pages; we report the *preceding* page so a chunk that visually spans pages 2-3 reports `page_first = 2`.

### Config: embedded defaults + partial overlay
`config.example.toml` is `include_str!`'d into the binary as the source of defaults. User configs are parsed as a `PartialConfig` and merged on top, so missing keys/sections fall back to known-good values. **When adding a new config field**: update `Config`, `PartialConfig`, the corresponding `merge_*` fn, *and* `config.example.toml` — the embedded-defaults parse will panic at startup if the example file doesn't deserialize as a complete `Config`.

Relative `db_path` values are resolved against the config file's directory; embedded-defaults-only mode leaves them relative to CWD.

### Logging
Both client and server append to one shared file (`[logging].file`, default `/tmp/document-search.log`) via `tracing_appender::rolling::never`, which uses `O_APPEND` — so writes under 4096 bytes are atomic across processes. Server also logs to stderr; client doesn't (the indicatif spinner owns stderr). The `Role` enum + PID prefix in every log line is what disentangles the two streams in the shared file.

### Error surfacing
`main.rs` unwraps `Error::Client(ClientError::Server(msg))` and prints `msg` directly — server-originated errors arrive pre-formatted and shouldn't be re-wrapped with `client:` / `server reported error:` prefixes. Keep server-side error `Display` impls human-readable for this reason.
