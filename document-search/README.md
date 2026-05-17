# document-search

Local document search: turso (SQLite/libSQL) for storage, Ollama for embeddings,
native vector search via `vector_distance_cos` on `F32_BLOB(N)` columns. Same
binary is both client and server — CLI invocations connect over a Unix socket
and auto-spawn a detached server if one isn't running.

## Build

```sh
cargo build --release
```

## Configure

Config is loaded from `--config <PATH>`, then `./config.toml`, then
`~/.config/document-search/config.toml` (first one that exists). Copy the
example into place and edit it:

```sh
mkdir -p ~/.config/document-search
cp config.example.toml ~/.config/document-search/config.toml
```

Relative `db_path` values are resolved against the config file's directory.

## Prerequisites

[Ollama](https://ollama.com) running locally, with embedding and chat models
pulled:

```sh
ollama pull qwen3-embedding:8b      # embeddings (configured under [ollama])
ollama pull qwen2.5:14b             # chat model used for summarization
```

The first time the binary runs it probes the embedding model for its vector
length and caches the result in the DB; subsequent runs skip the probe.

For PDFs, `pdftotext` and `pdftoppm` (poppler-utils) are required; OCR fallback
on garbled pages additionally needs `tesseract`.

## Commands

The interesting ones, in roughly the order you'll use them.

### `ingest` — add a document

Chunks the file, embeds each chunk via Ollama, then builds a hierarchical
summary tree using the configured chat model. Without `--detach` the client
blocks with a spinner until the job finishes; with `--detach` the job is
queued and the client returns immediately.

```sh
# Foreground: spinner + progress, exits when ingest + summary are done.
document-search ingest '/home/jeff/scratch/games/source_material/free_or_stolen/Exalted 2E/Exalted 2E.pdf'

# Detached: queue a few large PDFs and let the server crunch through them.
document-search ingest --detach '/home/jeff/scratch/games/source_material/free_or_stolen/Exalted 2E/Exalted 2E.pdf'
document-search ingest --detach '/home/jeff/scratch/games/source_material/free_or_stolen/Exalted 2E/Books of Sorcery Vol. 2 - White and Black Treatises.pdf'
document-search ingest --detach '/home/jeff/scratch/games/source_material/free_or_stolen/World of Darkness (Classic)/v20 Vampire The Masquerade - 20th Anniversary Edition.pdf'

# Skip the summary phase (faster, but `search --include-summaries` won't work
# on this document until you re-ingest).
document-search ingest --no-summary path/to/doc.pdf

# Override chunking and summary depth for a single ingest.
document-search ingest --chunk-size 8000 --overlap 1500 --max-depth 3 path/to/doc.pdf
```

### `search` — vector search

Requires exactly one scope: either `--path <exact ingested path>` or one or
more `--tag <tag>` (use `--match-all` to require all of them).

```sh
# Search inside one document.
document-search search 'sorcery initiation' \
  --path '/home/jeff/scratch/games/source_material/free_or_stolen/Exalted 2E/Exalted 2E.pdf'

# Search across all docs tagged "wod".
document-search search 'clan disciplines' --tag wod

# Require both tags, return up to 10 hits per doc, drop weak matches.
document-search search 'celerity dots' --tag wod --tag vampire --match-all \
  --limit 10 --cutoff 0.4

# Full chunk text instead of truncated snippets. Useful for piping into other
# tools or reading in full.
document-search search 'thaumaturgy paths' --tag wod --no-truncate

# Also vector-search the per-document summary tree and group results by doc
# with a "region summary" alongside the chunk hits.
document-search search 'how do disciplines work' --tag wod --include-summaries

# Machine-readable output — the entire stdout is a single JSON object.
document-search search 'sorcery' --tag exalted --output-mode json | jq '.results[0]'
```

```
search "sorcery initiation" — 5 result(s) (cutoff 0.300, limit 5/doc)

/.../Exalted 2E.pdf  page 142-143  similarity 0.6821
  Sorcery is the art of channeling Essence through… (truncated)
```

### `queue` — manage the worker

The server runs one job at a time off an mpsc queue. `status`, `list`,
`cancel`, and tag-list bypass the queue and stay responsive while a long
ingest is running.

```sh
# Snapshot of the current job + everything queued (bypasses the queue).
document-search status

# Live-updating dashboard, redraws every 500ms by default.
document-search status --watch
document-search status --watch --interval-ms 1000

# Just the queue, no extra status formatting.
document-search queue list

# Remove a queued job (or cancel the running one) by id. Accepts the short
# 8-char prefix shown by `status` / `queue list`.
document-search queue delete 3f9a1c2b

# Cancel the current cancellable job (only ingests are cancellable) and drop
# every queued job.
document-search queue clear

# Repair orphaned rows left behind by interrupted summarize runs.
document-search queue cleanup
```

Typical `status` output during a backlog:

```
uptime: 4m12s

in progress:
  [3f9a1c2b] ingest /.../Exalted 2E.pdf (running 47s, embedding 142/318)

queued (2):
  1. [7c4e8d91] ingest /.../Books of Sorcery Vol. 2 ... (queued 12s)
  2. [b201f3a5] ingest /.../v20 Vampire The Masquerade ... (queued 8s)
```

### Other commands

```sh
document-search list                                       # everything ingested + queued
document-search list --tag wod --tag vampire --match-all   # filter by tag(s)
document-search info '/.../Exalted 2E.pdf'                 # metadata for one doc
document-search text --pages 142 145 '/.../Exalted 2E.pdf' # slice of original text
                                                           # (also --bytes / --chars)
document-search tag add '/.../Exalted 2E.pdf' exalted 2e   # tags: lowercased + trimmed
document-search tag list
document-search delete '/.../Exalted 2E.pdf'               # drops chunks + embeddings + tags
document-search task-log --limit 20                        # recent server task history
document-search server                                     # run server in foreground
document-search print-config                               # merged defaults + overrides
```

Every subcommand accepts `--output-mode json` for a single-object JSON
payload, suitable for `jq`.

## Logging

Both client and server append to `[logging].file` (default
`/tmp/document-search.log`). Override the default `warn,document_search=trace`
filter via `RUST_LOG=document_search=debug`.

## Reset

Kill any running server, remove its socket, and wipe the DB:

```sh
killall -9 document-search 2>/dev/null
rm -f /tmp/document-search.sock
rm -f document-search.db document-search.db-wal document-search.db-shm
```

The `rm` paths assume the default `db_path` from `config.example.toml`; adjust
them if you've configured a different `db_path`.
