# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build / run / test

```sh
cargo build --release
cargo run -- <subcommand>                    # uses ./config.toml or ~/.config/document-search/config.toml
cargo run -- --config ./other.toml <sub>     # explicit override (errors if missing)
cargo test                                   # all unit tests
cargo test config::                          # one module (binary-only crate: no --lib)
cargo test split_sql_handles_basic           # one test by name
RUST_LOG=document_search=debug cargo run     # default is `warn,document_search=trace`
```

Runtime prerequisites (unit tests don't need them; real ingests do): Ollama running locally with the embedding model from `[ollama]` and the chat model used for summaries pulled; `pdftotext` + `pdftoppm` (poppler-utils) for PDFs; `tesseract` for the per-page OCR fallback, plus a language pack per OCR language (`tesseract-data-<code>` on Arch, `tesseract-ocr-<code>` on Debian) and `osd` when `[ocr].language = "auto"`. Japanese OCR (default engine `ensemble`) instead needs `uv` and `git` (the server installs NDLOCR-Lite and YomiToku itself) and the vision model `[ocr].arbiter_model` pulled.

Reset state (kill server, drop socket, wipe DB — the `rm` paths assume the default `db_path`):
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
`src/server/mod.rs::worker_loop` pulls one `Job` at a time off an mpsc channel and runs ingests, info lookups, text reads, etc. serially. **`Status`, `Info`, `Summaries`, `List`, `QueueList`, `QueueDelete`, `QueueClear`, `QueueCleanup`, `TagList`, `TaskLog`, `OcrToolsStatus`, and `Export` without `--to` bypass the queue** — they're handled inline by the per-connection task in `src/server/connection.rs` so they stay responsive while a long ingest is running. The worker's `unreachable!()` branches enforce that. Mutating tag operations (`TagAdd`, `TagRemove`) and `Delete` go through the worker because they write through the shared connection.

`info` also reports queued/running jobs that will change its document. Each `QueueEntry` / `RunningJob` carries `target: Option<(PendingEffect, PathBuf)>` from `Request::pending_effect` (ingest → create; summarize/tag add/tag remove → update; delete → delete), with the path run through `commands::normalize_path`, which works for files that don't exist (yet or any more). `info` succeeds for a path that isn't ingested as long as some pending job targets it. The worker moves a job from `queued` to `current` under one lock so `info` never misses it in between. The client turns every request path into an absolute one (`main.rs::absolutize_paths`) because the long-lived server's CWD is whichever client spawned it.

`ServerState` (queue + current job + shared `Arc<Db>`) is behind a `std::sync::Mutex`; **only lock for short sync sections, never across `.await`** — the codebase relies on this to keep status snapshots cheap. The worker also spawns a per-job relay task (`run_progress_relay`) that feeds every `Progress` event into `RunningJob.tracker` (`src/server/tracker.rs`), which enriches it with an overall weighted step count + ETA (including the projected summary-tree cost for ingests). The relay re-sends the latest envelope on a 1s heartbeat and logs it at info level; `status` rebuilds it at snapshot time. All three render via `src/progress_format.rs`, so the foreground spinner, `status`, and log lines use identical wording — change wording there, not in the client.

Cooperative cancel: each running job gets a `watch::Sender<bool>`. Only `Ingest`, `Summarize`, `Export` (with `--to`), and `OcrToolsInstall` poll it (ingest between chunks during embedding and by killing OCR subprocesses; ingest/summarize/export inside their LLM calls via `llm::complete`'s cancel-watch `select!`, export also between windows and seams; installs by killing `git`/`uv`); other handlers ignore it. Hence the `is_cancellable` flag on `RunningJob` and the targeting logic in the `QueueDelete` / `QueueClear` handlers. `QueueDelete` of a queued (not-yet-running) job is implemented by adding its id to `ServerState.cancelled_ids`; the worker checks that set when it dequeues and short-circuits with a `Final{ok:false}`.

### DB layout: per-dimension vector tables
`src/db/mod.rs::open` resolves the embedding model's vector length once (probing Ollama and caching in `embedding_model_dimensions`), then creates two parallel per-dimension tables with `F32_BLOB(N)` columns so turso's native `vector_distance_cos` works against them:
- `chunks_<N>` — 1:1 with `document_chunk` keyed by `chunk_id` (cascaded delete). Use `db.chunks_table`.
- `summary_vectors_<N>` — 1:1 with `document_summary` keyed by `summary_id` (cascaded delete). Use `db.summary_vectors_table`.

Switching embedding models with a different vector length means a new pair of tables appears; the old ones are left in place. Code that reads embeddings should always go through `db.chunks_table` / `db.summary_vectors_table` — never hard-code a name. `src/db/vector.rs::{to_blob, from_blob}` is the only `f32 ↔ bytes` boundary; everything else operates on `&[f32]`.

`document.language` is the ISO 639-1 code `language::detect` (whatlang) assigns at ingest, or NULL when detection was unreliable. `src/language.rs` is the single place that maps whatlang's 639-3 codes to the stored form and resolves summary-language lists.

Tags live in `document_tag (document_id, tag)` with `ON DELETE CASCADE`, so removing a document drops its tags automatically. Tags are normalized to lowercase + trimmed at the command layer before they hit the DB; queries (e.g. `list --tag` with `--match-all`) assume the stored form is already canonical.

`task_log` records every worker-handled job (`ingest`, `search`, `tag-add`, …) start→finish with status and error. Inline-handled requests (status, info, summaries, list, queue-*, tag-list, task-log, ocr-tools status, untranslated export) intentionally don't log — see `Request::task_metrics`. `Db::task_log_repair_abandoned()` runs once at server start to mark any `in-progress` rows left by a crashed/killed run as `failure`.

### Single shared `Connection` and the `fresh_conn` escape hatch
The server holds one `turso::Connection` (`db.conn`) and ingests open a transaction on it via raw `BEGIN`/`COMMIT`/`ROLLBACK` strings (turso 0.4 has no async transaction handle). Anything reading concurrently with an in-flight ingest **must** call `Db::fresh_conn()` to get its own connection — `commands::list_text` does this so the inline `list` handler doesn't land inside the worker's open transaction. New read-only inline handlers should follow the same pattern.

### Migrations
`src/db/migrations.rs` is a tiny embedded runner. Each migration is `include_str!`'d from `src/db/migrations/<NNNN>_<name>.sql`. To add one: drop a new file in that directory, append a `(version, name, include_str!(...))` tuple to `MIGRATIONS`, and let monotonic version ordering handle ordering. The runner wraps each file in `BEGIN`/`ROLLBACK` and uses a hand-rolled SQL statement splitter (`split_sql`) — semicolons inside single-quoted strings and `--` line comments are handled, but **multi-statement DDL with `BEGIN…END` blocks or dollar-quoting is not**.

The base schema (document, document_chunk, document_page, document_tag, document_summary, embedding_model_dimensions, task_log) lives entirely in `0001_initial.sql` — earlier intermediate migrations were squashed before any real users existed.

### Hierarchical summary tree
`src/summarize.rs` builds a tree of `document_summary` rows per document. Level 0 groups `cfg.summarize.group_size` consecutive `document_chunk` rows, runs them through `llm::complete`, and embeds the resulting summary into `summary_vectors_<N>`. Level k > 0 does the same against level-(k-1) summaries until the level fits in a single node or `cfg.summarize.max_depth` is reached. Each summary's `content_hash` is the sha256 of its concatenated input texts and is the dedup key inside `(document_id, language, level)` — re-running `summarize` on an unchanged document is a no-op (zero LLM calls). `ingest` calls `summarize` as its final phase unless `--no-summary` is passed; the `summarize` subcommand (`Request::Summarize`) runs it alone against an already-ingested document.

The summary phase runs *after* the ingest transaction commits, so a summarize failure leaves the document, chunks, and embeddings in place. `ingest` reports that as `IngestError::SummarizeAfterIngest`; the worker records it in `task_log` with status `partial` (not `failure`) and logs "job: partially succeeded", and the error text tells the user to run `summarize`.

**One independent tree per output language.** `[summarize].languages` (or repeated `ingest --summary-lang`) is resolved by `language::resolve_summary_languages` — `"source"` becomes `document.language` (fallback `en`), duplicates collapse — and `build_tree` runs once per language with a "write in X" line appended to the system prompt. Every `document_summary` row carries `language` (migration 0002 backfilled pre-existing rows to `'en'`); **any query that walks the tree (parent/child, levels, partial-level detection) must filter on `language`**, or two trees get mixed. The `Summarizing` progress event carries `tree`/`trees`/`language`, and `JobTracker` keeps per-tree counters.

Each group's user turn is the input fenced in `<document>` tags followed by the instruction restated (`user_message`); the hash covers only the input. `summarize_group` then checks the output with `language::detect` (parentheticals stripped, since cross-language summaries carry source-script names) and retries once with a stricter reminder before failing with `WrongLanguage`. `summarize --rebuild` skips the hash lookup, feeds each level from the rows just built (in memory, since the DB still holds the old tree), and deletes the old tree's rows only after every language tree has succeeded.

Cancellation/error rolls back every row inserted *during this run* via `summarize::delete_summary_rows`, so a cancel leaves the DB exactly as it was. The next run's per-group hash skip then carries you forward.

`summaries <path>` (`src/summaries.rs`, an inline bypass) reads one language tree whole through `fresh_conn` and filters it in memory: `--level`, `--index` (0-based within a level), `--pages`/`--bytes` (overlap), `--outline` for structure without text. With no `--lang` it uses the only tree, else `language::preferred_summary_language` (the same default `search` uses), whose config inputs are copied into `ServerState.summary_prefs` because inline handlers don't hold the `Config`.

`search --include-summaries` vector-ranks summaries (all languages — embeddings are multilingual) alongside chunks and groups results by document with a "region summary" line per hit, preferring `[search].summary_language` (default: the first resolved `[summarize].languages` entry) and falling back to any language.

### Export (`src/export/`)
`export <path> [--output-txt P] [--output-md P] [--output-pdf P] [--to <lang>] [--window-chunks N] [--overlap-chunks N]` writes a whole ingested document out. `Request::Export` is routed by `--to`: **without it the connection task answers inline** (`connection.rs`, reading through `Db::fresh_conn()`, PDF font settings copied into `ServerState.export`); **with it the worker runs it** as a cancellable job (in the `is_cancellable` list, with a relay + `cancel_rx` like `Summarize`, and a `task_log` row — `Request::task_metrics` returns `None` for the untranslated case). Both go through `export::handle`, which applies `[export]` defaults and CLI overrides (`Params::new`, window validation only when translating), runs, and formats text/JSON. `pending_effect` is `None`: export is read-only, so `info` never lists it.

The document text isn't stored whole. `export::run` loads every `document_chunk` in order and `export::stitch` rebuilds text by cursor over the canonical byte offsets, counting chunk overlap once (same rule as `commands::stitch_bytes`, but over a `&turso::Connection` so it can use a fresh connection). Page-separator form feeds stay in the text; `render::to_txt` turns them into blank lines and `blocks::parse_blocks` treats them as paragraph breaks.

**Structure is heuristic only.** `blocks::parse_blocks` turns text into `Block::{Heading{level}, Paragraph, ListItem}`: blank lines/form feeds end paragraphs, wrapped lines are re-joined (no space between CJK characters, line-broken hyphens rejoined), and a *single-line* paragraph is a heading only if it is short, has no terminal punctuation, and matches `Chapter/Part N` (1), `Section` (2), `第…章/部/編` (1), `第…節` (2), `N.`/`N.N`/`N.N.N` + title (2/3/4), or ALL-CAPS (2). Markdown (`render::to_markdown`, with escaping so prose never turns into syntax) and PDF both render these blocks, so they agree. Txt skips block parsing to stay faithful. The LLM is never asked for structure.

**PDF** (`pdf.rs`) is pure Rust: layout is ours (`wrap`/`layout`: greedy line breaking over `ttf-parser` advances, breaking at spaces or between CJK characters with minimal kinsoku, A4, headings kept with the next lines) and `krilla` draws and subsets the font. The font is `[export].pdf_font` (+ `pdf_font_index` for `.ttc`), else the Noto Sans / Noto Sans CJK JP fonts embedded from `assets/fonts/` via `include_bytes!` (the CJK one only when the text has CJK). Characters the font lacks are counted and logged once; nothing is substituted. Rendering runs in `spawn_blocking`.

**Translation** (`translate.rs`). `windows(n, window, overlap)` gives chunk ranges advancing by `window - overlap`; each window's source is the stitched text of its chunks. Every window is translated first (`Translating{current,total,seams}` progress), then seams are merged (`Seaming{current,total}`); the two phases are sequential on purpose because `JobTracker` phases are ordered. Each translation goes through `summarize::complete_ladder` (the thinking-spiral ladder shared with summaries) and is checked with `language::detect`; a wrong-language answer is retried once with a sterner reminder, then fails with `WrongLanguage`. The seam pass walks an accumulator: it cuts the tail of the accumulated text and the head of the next translation (about 1.5× the shared fraction, snapped to paragraph, then line, then sentence boundaries), asks the LLM to merge `<a>…</a><b>…</b>` so each piece appears once, and splices the result back. The merge is only accepted if it has no tags, is in the target language, and is between 0.6× the longer input and 1.1× the sum of both; two failures keep both passages (**duplicate rather than lose** — the model collapses legitimately repeated paragraphs, so this guard matters). `--overlap-chunks 0` skips the seam pass and concatenates. Nothing is written until the whole job succeeds, so cancel/error leaves no output.

**Output.** `Request::Export.outputs` is a list of `ExportOutput{format,path}`, so one (possibly translated) text is rendered into every requested format — translation is non-deterministic, which is why this is one run rather than several. The CLI builds it from `--output-txt/--output-md/--output-pdf`; path `-` means `path: None` (returned as `Event::Output`; at most one, never PDF); no flag means txt to stdout. `Params::new` also rejects duplicate paths. `run` renders everything first, then writes each file with `write_atomic` (temp file in the destination directory, mode of the replaced file or 0644, then rename), so a render failure writes nothing; client paths are made absolute by `absolutize_paths`. JSON mode returns `{ok,path,source_language,language,translated,translation_skipped,outputs:[{format,path,bytes}],text}`; if you change it, update the skill.

### turso 0.4 self-referential cascade gotcha
`document_summary.parent_id` is a self-reference with `ON DELETE CASCADE`. Turso 0.4's cascade implementation **overflows the runtime stack on any delete against this table**, even for a single row with no children. Both `summarize::delete_summary_rows` (cancel/error path) and `cleanup::repair_document` (startup repair of partial trees) work around this by opening a dedicated connection with `PRAGMA foreign_keys = OFF` and manually deleting dependents in order: `summary_vectors_<N>` row → null out children's `parent_id` → delete the row. Any new code that deletes from `document_summary` must do the same dance.

`cleanup::scan_and_repair` runs once at server start. It detects per-`(document, language)` summary trees with a "partial" level (mixed NULL/non-NULL `parent_id` at the same level — the signature of an interrupted pre-rollback summarize run) and drops everything from the partial level upward so the user can re-ingest. The CLI exposes the same routine as `queue cleanup`.

### LLM provider abstraction
`src/llm.rs::complete` issues a single chat completion against `cfg.llm.provider` (`ollama` → `/api/chat`, or `anthropic` → `/v1/messages`). It takes an optional `watch::Receiver<bool>` and runs the HTTP call inside a `tokio::select!`; on cancel it drops the in-flight request and returns `LlmError::Cancelled`. Anthropic mode reads the API key from the env var named in `cfg.anthropic.api_key_env` (not the value — the name). An `llm::Overrides` argument can replace `think` / `max_output_tokens` per call. Ollama calls always send `num_ctx = [ollama].num_ctx`, a fixed value because a changed `num_ctx` makes Ollama reload the model. Without it Ollama runs a ~4K context and silently drops the *front* of longer prompts, system prompt included. `num_predict` is shrunk so the estimated prompt (`llm::estimate_tokens`, deliberately pessimistic) plus output fits, a prompt that can't fit fails with `PromptTooLarge`, and a response whose `prompt_eval_count` shows Ollama cut the prompt fails with `PromptTruncated`. Only `summarize::complete_ladder` calls it (via `complete_with_fallback` for summaries and directly from `export` for translation): on `LlmError::ThinkingExhausted` it walks `ladder_step` (fallback prompt at 1/4 budget, then fallback prompt with `think = false`) so one thinking spiral doesn't fail and roll back the whole ingest.

### Wire envelope and per-command output mode
Every request is wrapped in `protocol::RequestEnvelope { output_mode, request }`. `OutputMode::{Text, Json}` propagates from `--output-mode` on the CLI through the socket to the worker, so command implementations come in pairs (`commands::tag_add` / `commands::tag_add_json`, `commands::search_text` / `commands::search_json`, …). **When adding a subcommand, plumb both formatters** and dispatch on `OutputMode` in the relevant worker / inline-handler arm. The envelope's `output_mode` defaults to `Text`, so an old client that serialized the bare `Request` shape still parses (covered by `missing_output_mode_defaults_to_text` in `protocol.rs`).

`.claude/skills/document-search/SKILL.md` is a Claude skill that drives this CLI through `--output-mode json` (`list`, `tag list`, `info`, `search`, `text`, `summaries`, `status`, `task-log`) and documents their JSON shapes. If you change a JSON output's shape, update that skill too.

### Canonical PDF text and offset invariant
Ingest accepts only `.txt` and `.pdf` (dispatched by extension in `src/ingest.rs`). For PDFs, `pdf::select_ocr_pages` picks pages to re-extract by OCR (which engine depends on the language; see "OCR paths" below): pages below the `[ocr]` `decodability_score` threshold (garbled), sparse pages when at least `sparse_page_ratio` of a ≥`sparse_min_pages`-page document is sparse (image-only scans come back *empty*, not garbled), every page when the text layer is entirely empty, or every page with `ingest --force-ocr`. The score's allowlist is ASCII plus the CJK/Hangul ranges implied by the tesseract language spec (`ScriptSet`); it never admits Latin-1, which is the CID-garbage signal. `[ocr].language = "auto"` (or `--ocr-language auto`) resolves the spec from tesseract OSD on the first candidate page. CJK tesseract output goes through `collapse_cjk_spaces` to drop tesseract's inter-character spaces. Offsets are computed only *after* OCR substitution, and an extraction with no text at all is an error. `src/pdf.rs` then joins the final pages with a U+000C form-feed separator into one canonical `full_text`. **All byte/char offsets in the DB (`document_chunk`, `document_page`, totals) are computed against this exact string** — so chunkers, byte-range readers, and page lookups all share one coordinate system. If you change the separator or the page-join logic, every downstream offset breaks.

`page_for_byte_or_preceding` exists because chunks can start in the separator gap between pages; we report the *preceding* page so a chunk that visually spans pages 2-3 reports `page_first = 2`.

### OCR paths
`pdf::extract` has one page-selection step and then several OCR paths for the selected pages. Every path must return exactly one string per selected page, in order; `extract` substitutes them into the page list and computes offsets afterwards, so the offset invariant above holds whichever path ran.

1. **No OCR**: pages pdftotext read well keep its text.
2. **Tesseract** (`pdf::ocr_page`, one page at a time): any resolved spec without `jpn`/`jpn_vert`, or any spec when `[ocr].japanese_engine = "tesseract"`. Language packs are checked up front (`check_langs_installed`).
3. **Japanese engines** (`ocr_ja::run`, batches): the resolved spec contains `jpn`/`jpn_vert` (`ocr_ja::handles`) and `japanese_engine` is `ensemble`, `ndlocr`, or `yomitoku`. `ndlocr` and `yomitoku` return that engine's text. `ensemble` runs both and merges them per page (below). No tesseract packs are checked on this path.

The spec is the `--ocr-language` override, else `[ocr].language`. `"auto"` is resolved first by tesseract OSD (`detect_ocr_language`), so tesseract is still required for `auto` even when the Japanese engines end up doing the OCR. The post-OCR decodability check and the `max_undecodable_page_ratio` tolerance apply to every path. `ExtractProgress::Stage` carries the engine-install stages (`ocr-install-ndlocr`, `ocr-install-yomitoku`), whose wording lives in `progress_format.rs`.

### Japanese OCR engines
When the resolved OCR spec contains `jpn`/`jpn_vert` and `[ocr].japanese_engine` isn't `tesseract`, `pdf::extract` hands the selected pages to `src/ocr_ja.rs` instead of the tesseract loop (tesseract is then needed only for `auto`'s OSD). It renders pages in batches of `[ocr_tools].batch_pages` via `pdf::rasterize_page_to` and runs NDLOCR-Lite (CPU) and YomiToku (GPU) concurrently over each batch. For `ensemble`, `src/ocr_merge.rs` diffs the two readings per page (whitespace ignored, B = YomiToku supplies the paragraph breaks), settles known patterns with `rule_choice`, and sends the remaining hunks plus the page PNG to `[ocr].arbiter_model` via `llm::complete_ext`. It asks twice, with A/B swapped, and `reconcile` keeps only answers that agree in both orders. The rules and the neutral prompt come from replaying the benchmark: the model followed engine descriptions over the image and leaned towards the second reading. Engine/install failures fail the ingest; only an unusable arbiter reply degrades to `Hunk::default_choice`.

`src/ocr_tools.rs` installs the engines under `[ocr_tools].dir` with `git` + `uv` (`ensure`), in place, writing `<tool>/.installed.json` as the *last* step. A dir without a stamp matching the configured version/Python is deleted and rebuilt, so never treat a dir's existence as "installed". Installs only run in the worker (lazily from an ingest, or `ocr-tools install`), so they can't race; `ocr-tools status` is an inline bypass. Every step's output goes to `install.log`, and each `ToolsError` variant names the failed step plus a hint. External programs everywhere go through `src/proc.rs::run` (cancel kills the child).

### Config: embedded defaults + partial overlay
`config.example.toml` is `include_str!`'d into the binary as the source of defaults. User configs are parsed as a `PartialConfig` and merged on top, so missing keys/sections fall back to known-good values. **When adding a new config field** (see `[export]` for the newest example, including its cross-field `validate_export_windows`): update `Config`, `PartialConfig`, the corresponding `merge_*` fn, *and* `config.example.toml` — the embedded-defaults parse will panic at startup if the example file doesn't deserialize as a complete `Config`.

Relative `db_path` values are resolved against the config file's directory; embedded-defaults-only mode leaves them relative to CWD.

### Logging
Both client and server append to one shared file (`[logging].file`, default `/tmp/document-search.log`) via `tracing_appender::rolling::never`, which uses `O_APPEND` — so writes under 4096 bytes are atomic across processes. Server also logs to stderr; client doesn't (the indicatif spinner owns stderr). The `Role` enum + PID prefix in every log line is what disentangles the two streams in the shared file.

### Error surfacing
`main.rs` unwraps `Error::Client(ClientError::Server(msg))` and prints `msg` directly — server-originated errors arrive pre-formatted and shouldn't be re-wrapped with `client:` / `server reported error:` prefixes. Keep server-side error `Display` impls human-readable for this reason.
