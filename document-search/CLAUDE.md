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
`src/server/mod.rs::worker_loop` pulls one `Job` at a time off an mpsc channel and runs ingests, info lookups, text reads, etc. serially. **`Status`, `List`, `QueueList`, `QueueDelete`, `QueueClear`, `QueueCleanup`, `TagList`, and `TaskLog` bypass the queue** — they're handled inline by the per-connection task in `src/server/connection.rs` so they stay responsive while a long ingest is running. The worker's `unreachable!()` branches enforce that. Mutating tag operations (`TagAdd`, `TagRemove`) and `Delete` go through the worker because they write through the shared connection.

`ServerState` (queue + current job + shared `Arc<Db>`) is behind a `std::sync::Mutex`; **only lock for short sync sections, never across `.await`** — the codebase relies on this to keep status snapshots cheap. The worker also spawns a per-job relay task that taps the event stream and records the latest `Progress` event on `RunningJob.latest_progress`, so the inline `status` handler can show live progress without going through the worker.

Cooperative cancel: each running job gets a `watch::Sender<bool>`. Currently only `Ingest` polls it (between chunks during embedding, and again inside its post-chunk summarize phase via `llm::complete`'s cancel-watch `select!`); other handlers ignore it. Hence the `is_cancellable` flag on `RunningJob` and the targeting logic in the `QueueDelete` / `QueueClear` handlers. `QueueDelete` of a queued (not-yet-running) job is implemented by adding its id to `ServerState.cancelled_ids`; the worker checks that set when it dequeues and short-circuits with a `Final{ok:false}`.

### DB layout: per-dimension vector tables
`src/db/mod.rs::open` resolves the embedding model's vector length once (probing Ollama and caching in `embedding_model_dimensions`), then creates two parallel per-dimension tables with `F32_BLOB(N)` columns so turso's native `vector_distance_cos` works against them:
- `chunks_<N>` — 1:1 with `document_chunk` keyed by `chunk_id` (cascaded delete). Use `db.chunks_table`.
- `summary_vectors_<N>` — 1:1 with `document_summary` keyed by `summary_id` (cascaded delete). Use `db.summary_vectors_table`.

Switching embedding models with a different vector length means a new pair of tables appears; the old ones are left in place. Code that reads embeddings should always go through `db.chunks_table` / `db.summary_vectors_table` — never hard-code a name. `src/db/vector.rs::{to_blob, from_blob}` is the only `f32 ↔ bytes` boundary; everything else operates on `&[f32]`.

Tags live in `document_tag (document_id, tag)` with `ON DELETE CASCADE`, so removing a document drops its tags automatically. Tags are normalized to lowercase + trimmed at the command layer before they hit the DB; queries (e.g. `list --tag` with `--match-all`) assume the stored form is already canonical.

`task_log` records every worker-handled job (`ingest`, `search`, `tag-add`, …) start→finish with status and error. Inline-handled requests (status, list, queue-*, tag-list, task-log) intentionally don't log — see `Request::task_metrics`. `Db::task_log_repair_abandoned()` runs once at server start to mark any `in-progress` rows left by a crashed/killed run as `failure`.

### Single shared `Connection` and the `fresh_conn` escape hatch
The server holds one `turso::Connection` (`db.conn`) and ingests open a transaction on it via raw `BEGIN`/`COMMIT`/`ROLLBACK` strings (turso 0.4 has no async transaction handle). Anything reading concurrently with an in-flight ingest **must** call `Db::fresh_conn()` to get its own connection — `commands::list_text` does this so the inline `list` handler doesn't land inside the worker's open transaction. New read-only inline handlers should follow the same pattern.

### Migrations
`src/db/migrations.rs` is a tiny embedded runner. Each migration is `include_str!`'d from `src/db/migrations/<NNNN>_<name>.sql`. To add one: drop a new file in that directory, append a `(version, name, include_str!(...))` tuple to `MIGRATIONS`, and let monotonic version ordering handle ordering. The runner wraps each file in `BEGIN`/`ROLLBACK` and uses a hand-rolled SQL statement splitter (`split_sql`) — semicolons inside single-quoted strings and `--` line comments are handled, but **multi-statement DDL with `BEGIN…END` blocks or dollar-quoting is not**.

The base schema (document, document_chunk, document_page, document_tag, document_summary, embedding_model_dimensions, task_log) lives entirely in `0001_initial.sql` — earlier intermediate migrations were squashed before any real users existed.

### Hierarchical summary tree
`src/summarize.rs` builds a tree of `document_summary` rows per document. Level 0 groups `cfg.summarize.group_size` consecutive `document_chunk` rows, runs them through `llm::complete`, and embeds the resulting summary into `summary_vectors_<N>`. Level k > 0 does the same against level-(k-1) summaries until the level fits in a single node or `cfg.summarize.max_depth` is reached. Each summary's `content_hash` is the sha256 of its concatenated input texts and is the dedup key inside `(document_id, level)` — re-running `summarize` on an unchanged document is a no-op (zero LLM calls). `ingest` calls `summarize` as its final phase unless `--no-summary` is passed.

Cancellation/error rolls back every row inserted *during this run* via `summarize::rollback_inserted`, so a cancel leaves the DB exactly as it was. The next run's per-group hash skip then carries you forward.

`search --include-summaries` vector-ranks summaries alongside chunks and groups results by document with a "region summary" line per hit.

### turso 0.4 self-referential cascade gotcha
`document_summary.parent_id` is a self-reference with `ON DELETE CASCADE`. Turso 0.4's cascade implementation **overflows the runtime stack on any delete against this table**, even for a single row with no children. Both `summarize::rollback_inserted` (cancel/error path) and `cleanup::repair_document` (startup repair of partial trees) work around this by opening a dedicated connection with `PRAGMA foreign_keys = OFF` and manually deleting dependents in order: `summary_vectors_<N>` row → null out children's `parent_id` → delete the row. Any new code that deletes from `document_summary` must do the same dance.

`cleanup::scan_and_repair` runs once at server start. It detects documents whose summary tree has a "partial" level (mixed NULL/non-NULL `parent_id` at the same level — the signature of an interrupted pre-rollback summarize run) and drops everything from the partial level upward so the user can re-ingest. The CLI exposes the same routine as `queue cleanup`.

### LLM provider abstraction
`src/llm.rs::complete` issues a single chat completion against `cfg.llm.provider` (`ollama` → `/api/chat`, or `anthropic` → `/v1/messages`). It takes an optional `watch::Receiver<bool>` and runs the HTTP call inside a `tokio::select!`; on cancel it drops the in-flight request and returns `LlmError::Cancelled`. Anthropic mode reads the API key from the env var named in `cfg.anthropic.api_key_env` (not the value — the name). Currently only `summarize::summarize_inner` calls it.

### Wire envelope and per-command output mode
Every request is wrapped in `protocol::RequestEnvelope { output_mode, request }`. `OutputMode::{Text, Json}` propagates from `--output-mode` on the CLI through the socket to the worker, so command implementations come in pairs (`commands::tag_add` / `commands::tag_add_json`, `commands::search_text` / `commands::search_json`, …). **When adding a subcommand, plumb both formatters** and dispatch on `OutputMode` in the relevant worker / inline-handler arm. The envelope's `output_mode` defaults to `Text`, so an old client that serialized the bare `Request` shape still parses (covered by `missing_output_mode_defaults_to_text` in `protocol.rs`).

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
