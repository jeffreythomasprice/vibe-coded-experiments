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
on garbled or scanned pages additionally needs `tesseract` plus a language pack
for each language you OCR. For non-English scans, e.g. Japanese on Arch:

```sh
sudo pacman -S tesseract tesseract-data-eng tesseract-data-jpn tesseract-data-jpn_vert \
  tesseract-data-osd                # osd is only needed for --ocr-language auto
```

Japanese pages are OCR'd with NDLOCR-Lite and YomiToku instead of tesseract
by default (see [Japanese OCR](#japanese-ocr)). The server installs those
itself on first use, which needs `uv` and `git` on PATH, about 7 GB of disk,
and ideally an NVIDIA GPU:

```sh
sudo pacman -S uv git
ollama pull qwen3.5:9b              # vision model that settles the engines' disagreements
```

PDF output from `export` uses Noto Sans and Noto Sans CJK JP, which are built
into the binary (`assets/fonts/`). Point `[export].pdf_font` at a font file only
for scripts those don't cover.

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
# on this document until you run `summarize` on it).
document-search ingest --no-summary path/to/doc.pdf

# Override chunking and summary depth for a single ingest.
document-search ingest --chunk-size 8000 --overlap 1500 --max-depth 3 path/to/doc.pdf
```

The document, chunks, and embeddings are committed before the summary phase
starts. If summarization fails or is cancelled, the document stays ingested:
the job is recorded in `task-log` as `partial` rather than `failure`, and you
can finish the summaries with `summarize` instead of re-ingesting.

### `summarize` — build or finish a summary tree

Builds the summary tree for a document that's already ingested, without
re-extracting, re-OCRing, or re-embedding it. Groups whose inputs haven't
changed are skipped, so this resumes after a failed summary phase, and adding a
`--summary-lang` adds a new tree without touching the existing ones.

```sh
# Retry after `ingest` reported "ingested ... but summarization failed".
document-search summarize path/to/scan.pdf

# Add a Japanese tree to a document that already has an English one.
document-search summarize --summary-lang ja path/to/scan.pdf

# Queue it and return immediately.
document-search summarize --detach path/to/scan.pdf
```

`--rebuild` regenerates every summary instead of skipping unchanged groups,
for example after switching to a better chat model or fixing bad summaries.
The new tree is built next to the old one, which is only deleted once every
tree has finished, so a failed or cancelled rebuild leaves the old summaries
in place.

```sh
document-search summarize --rebuild path/to/scan.pdf
```

Each summary is checked against its tree's language. One that comes back in
the wrong language is retried once with a stricter instruction, and if it is
still wrong the run fails instead of storing it. The whole group plus the
output budget has to fit in `[ollama].num_ctx`. The output budget is shrunk to
fit, and a prompt that is too big on its own, or one Ollama reports it
truncated, fails the run rather than being summarized from a fragment.

#### Non-English documents and cross-language summaries

The document's language is detected at ingest and stored (see `info`). OCR is
script-aware: set `[ocr].language` (or pass `--ocr-language`) to tesseract
codes like `jpn+eng`, or `auto` to let tesseract detect the script. Scanned
PDFs with no text layer are OCR'd automatically; `--force-ocr` OCRs every page
regardless.

Summaries can be built in several languages — one independent tree per
language. `source` means "the document's detected language", so
`["en", "source"]` gives an English tree plus a native one (and collapses to a
single tree for English documents). Set it in config:

```toml
[summarize]
languages = ["en", "source"]
```

or per ingest:

Japanese is read by its own engines rather than tesseract; see the next
section.

```sh
# Scanned Japanese PDF: detect the OCR script, then summarize in English and Japanese.
document-search ingest --ocr-language auto \
  --summary-lang en --summary-lang source path/to/scan.pdf

# Vertical Japanese text (common in books) — name the packs explicitly.
document-search ingest --ocr-language jpn_vert+jpn path/to/tategaki.pdf

# Already-clean text: no OCR involved, just the two summary trees.
document-search ingest --summary-lang en --summary-lang source path/to/neko.txt
```

```
$ document-search info path/to/neko.txt
path:             /.../neko.txt
status:           ingested
type:             txt
total_size_bytes: 2889
total_size_chars: 969
total_size_pages: null
language:         ja
summaries:        en (6), ja (6)
tags:             (none)
```

`info` answers immediately, even while an ingest is running, and lists any
queued or running jobs that will create, update, or delete that document. A
path that isn't ingested yet (or doesn't exist on disk yet) is reported as
long as a queued `ingest` will create it:

```
$ document-search info path/to/next.pdf
path:             /.../next.pdf
status:           not ingested
pending:
  queued #2  9f8e7d6c  will create  ingest /.../next.pdf
```

Embeddings are multilingual, so an English query finds Japanese chunks. With
`--include-summaries`, the region summary is shown in `[search].summary_language`
(default: the first entry of `[summarize].languages`) when one exists:

```
$ document-search search 'cat abandoned near a pond, hungry' --path path/to/neko.txt --include-summaries
=== /.../neko.txt ===
  region summary (level 1, en): The text describes a cat named "I" that was abruptly left on a
  straw mat and then thrown into a bamboo grove. The cat slowly navigates through the bamboo
  grove and encounters a large pond.
  - bytes 2157-2741  similarity 0.6634  [both]
      吾輩は藁の上から急に笹原の中へ棄てられたのである。 ようやくの思いで笹原を這い出すと向うに大きな池がある。…
```

#### Japanese OCR

When the OCR language includes Japanese (`jpn` or `jpn_vert`, given
explicitly or picked by `auto`), pages are read by two dedicated engines
instead of tesseract:

- [NDLOCR-Lite](https://github.com/ndl-lab/ndlocr-lite) (National Diet
  Library, CC BY 4.0) runs on the CPU.
- [YomiToku](https://github.com/kotaro-kinoshita/yomitoku) (CC BY-NC-SA 4.0,
  non-commercial use only) runs on the GPU.

On a benchmark of vertical-text novel pages, tesseract got 16% of characters
wrong, NDLOCR-Lite 0.2–0.6%, and YomiToku 0.9%. They rarely make the same
mistake: NDLOCR-Lite occasionally drops or repeats a line, while YomiToku
misreads dashes and lets furigana (reading glosses) into the text. So by
default both read every page, and their disagreements are settled in three
ways:

- The text they agree on is kept as is.
- Known patterns are settled by fixed rules. A difference that is only
  typographic (`--` versus `――`, character width) takes the full-width form.
  NDLOCR-Lite's dash against YomiToku's ー, `!`, or nothing is a dash. Short
  kana that only YomiToku has is furigana and is dropped.
- For anything else, mostly single-character misreads, `[ocr].arbiter_model`
  looks at the page image and picks one reading. It is asked twice, with the
  readings in both orders, and its answer is used only when both agree.
  Otherwise NDLOCR-Lite's reading is kept.

The model never sees, and so can't change, the text both engines agreed on.
On the benchmark this got 1 character wrong in about 4,000.

`[ocr].japanese_engine` picks `"ensemble"` (the default), `"ndlocr"`,
`"yomitoku"`, or `"tesseract"`. If an engine fails, the ingest fails rather
than falling back to tesseract. The exception is an arbiter reply that is
still unusable after a retry. That page then uses NDLOCR-Lite's readings,
keeps any text only YomiToku has, and logs a warning.

The engines are installed under `[ocr_tools].dir` (default
`~/.local/share/document-search/ocr-tools`) the first time an ingest needs
them. That is a git checkout of NDLOCR-Lite at a pinned commit, plus a `uv`
virtualenv for each engine; YomiToku pulls in torch and downloads its model
weights. Every step's output is appended to `install.log` in that dir, and a
failure names the step that failed and what to check. To install ahead of
time, or see what's there:

```sh
document-search ocr-tools install              # installs what's missing; a few minutes
document-search ocr-tools install --reinstall  # rebuild from scratch
document-search ocr-tools status
```

```
$ document-search ocr-tools status
dir:         /home/you/.local/share/document-search/ocr-tools
uv:          uv 0.13.1
git:         git version 2.56.0
NDLOCR-Lite: installed d001dad2013ceabb5736ff8d637474e443eb73c2 (Python 3.12, …)
YomiToku:    installed 0.15.0 (Python 3.12, CUDA available, …)
```

Which path a page takes depends on the resolved OCR language, meaning
`--ocr-language` if given, else `[ocr].language`, with `auto` detected by
tesseract first:

| Resolved language | `japanese_engine` | Pages are read by |
|---|---|---|
| no `jpn` / `jpn_vert` | any | tesseract |
| includes `jpn` or `jpn_vert` | `"tesseract"` | tesseract |
| includes `jpn` or `jpn_vert` | `"ndlocr"` / `"yomitoku"` | that engine alone |
| includes `jpn` or `jpn_vert` | `"ensemble"` (default) | both, merged as above |

`--force-ocr` works the same on every path. `auto` still needs tesseract and
`tesseract-data-osd` to detect the script; an explicit spec such as
`--ocr-language jpn_vert+jpn` doesn't need tesseract at all when a Japanese
engine handles it.

Settings, all optional:

| Key | Default | Meaning |
|---|---|---|
| `[ocr].japanese_engine` | `"ensemble"` | `ensemble`, `ndlocr`, `yomitoku`, or `tesseract` |
| `[ocr].arbiter_model` | `"qwen3.5:9b"` | Vision model for the ensemble (Ollama only; Anthropic uses `[anthropic].model`) |
| `[ocr_tools].dir` | `~/.local/share/document-search/ocr-tools` | Install location |
| `[ocr_tools].auto_install` | `true` | Install missing engines when an ingest needs them |
| `[ocr_tools].python` | `"3.12"` | Python for the engines' virtualenvs (uv downloads it if needed) |
| `[ocr_tools].ndlocr_repo` | NDL's GitHub repo | Where NDLOCR-Lite is cloned from |
| `[ocr_tools].ndlocr_rev` | the benchmarked commit | NDLOCR-Lite commit to check out |
| `[ocr_tools].yomitoku_version` | `"0.15.0"` | YomiToku release from PyPI |
| `[ocr_tools].yomitoku_device` | `"auto"` | `auto` (CUDA if the install found a GPU), `cuda`, or `cpu` |
| `[ocr_tools].batch_pages` | `32` | Pages rendered and passed to the engines per run |

Changing `ndlocr_rev`, `yomitoku_version`, or `python` reinstalls that engine
on next use; changing only `ndlocr_repo` doesn't, so follow it with
`ocr-tools install --reinstall`. With `auto_install = false`, an
ingest that needs a missing engine fails and asks you to run
`ocr-tools install`.

### `summaries` — read a summary tree

Prints the summaries `summarize` built for a document. Level 0 holds the leaf
summaries (each covers a group of chunks), and each level above summarizes the
one below, up to a single root. With no filters it prints the whole tree, top
level first.

```sh
# Structure only: levels, counts, and each summary's pages, bytes, parent and
# children (positions within the neighbouring levels).
document-search summaries --outline path/to/book.pdf

# The root summary (use the top level number from --outline).
document-search summaries --level 2 path/to/book.pdf

# Leaf summaries #3 to #5 (0-based, inclusive).
document-search summaries --level 0 --index 3 5 path/to/book.pdf

# Every summary, at any level, that overlaps pages 100-150 (or a byte range).
document-search summaries --pages 100 150 path/to/book.pdf
document-search summaries --level 0 --bytes 0 50000 path/to/notes.txt
```

`--index`, `--pages` and `--bytes` are mutually exclusive; `--index` needs
`--level`. `--lang <code>` picks a language tree when a document has several
(default: `[search].summary_language`, else the first resolved
`[summarize].languages` entry). Text output shows inclusive byte ranges you can
pass straight to `text --bytes`; JSON output (`--output-mode json`) uses the
stored half-open `byte_start`/`byte_end`, like `search`. The command bypasses
the queue, so it works while an ingest is running.

### `export` — write a whole document as txt, markdown or PDF

Writes an ingested document out in full, as plain text, markdown or PDF,
either as written or translated into another language by the LLM. Layout is
not preserved: tables and figures come out as broken text fragments, just as
they are in the OCR data. Markdown headings and bullet items are guessed from
the text itself (lines like `Chapter 3`, `2.1 Method`, `第一章`, or short
ALL-CAPS lines become headings; `•`/`・`/`-` lines become list items), and the
PDF is laid out from the same structure.

```sh
# Plain text to stdout (page breaks become blank lines).
document-search export notes.pdf > notes.txt

# Pick formats with --output-txt / --output-md / --output-pdf (any
# combination). "-" means stdout for txt and md; PDF always needs a file.
document-search export notes.pdf --output-md notes.md
document-search export notes.pdf --output-md - | less
document-search export notes.pdf --output-pdf notes.pdf

# Translate with the configured LLM. Runs as a queued, cancellable job
# (`status` shows progress, `queue delete <id>` cancels and writes nothing).
document-search export manual-ja.pdf --to en --output-md manual-en.md

# Translation is non-deterministic, so ask for every format in one run: they
# are all rendered from the same translation.
document-search export manual-ja.pdf --to en --output-txt manual-en.txt --output-md manual-en.md --output-pdf manual-en.pdf

# Skip the grammar-edit pass (see below): the text comes out exactly as
# stored, and the export is answered immediately.
document-search export notes.pdf --no-edit > notes.txt

# Translate bigger or smaller windows, with more or less overlap.
document-search export manual-ja.pdf --to en --window-chunks 6 --overlap-chunks 2 --output-md manual-en.md

# Queue it and return immediately (monitor with `status` / `queue list`).
# Every output must be a file: --detach with stdout is rejected up front.
document-search export manual-ja.pdf --to en --output-md manual-en.md --detach
```

`--to <code>` takes an ISO 639-1 code. If it is the document's own language
nothing is translated. An export that neither translates nor edits (`--no-edit`
without `--to`) only reads, so it is answered immediately even while an ingest
is running; any other export queues like an ingest.

`--detach` (`-d`) queues the export and returns immediately, like `ingest
--detach`; watch it with `status` or `queue list`, cancel it with `queue
delete`. It needs every output to be a file, because nothing is left to
receive stdout: `--detach` with `-` or with no `--output-*` fails straight away.
A detached export always queues, even one that would otherwise be answered
immediately.

By default the final text, translated or not, goes through a grammar-edit pass
(`[export].edit`; `--edit` / `--no-edit` override it per run). The LLM works
through the text in segments of about `[export].edit_segment_bytes` and fixes
only what OCR and chunking broke: lines split mid-sentence, hyphenated line
breaks, stray spaces before punctuation, commas and periods outside closing
quotes, dialogue run together in one paragraph. It is told not to reword
anything, and an edit that changes too much (or comes back in the wrong
language) is discarded and that segment is kept as it was, so a bad answer
costs a missed fix rather than lost text. All formats share the edited text.

Translation works through the document's chunks `[export].window_chunks` at
a time, each window sharing `[export].overlap_chunks` chunks with the next. A
second LLM pass then merges each seam so the shared text appears once. Raise
the overlap if seams read badly; set `overlap_chunks = 0` to skip the second
pass (the shared text then appears twice). Lower `--window-chunks` if the model
runs out of context. If a seam can't be merged reliably the export keeps both
passages (a duplicated paragraph rather than a lost one) and logs a warning.

Every format is rendered before any file is written, and each file is written
atomically, so a failed or cancelled export never leaves a partial file. With
`--output-mode json` the result is
`{ok, path, source_language, language, translated, translation_skipped, edited, edit_segments_kept, outputs: [{format, path, bytes}], text}`,
where `text` is the document when an output was `-` (stdout).

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

# Cancel the current cancellable job (ingest and summarize are cancellable) and drop
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
document-search export '/.../Exalted 2E.pdf' --output-md out.md  # whole doc as txt/md/pdf (see above)
document-search tag add '/.../Exalted 2E.pdf' exalted 2e   # tags: lowercased + trimmed
document-search tag list
document-search delete '/.../Exalted 2E.pdf'               # drops chunks + embeddings + tags
document-search task-log --limit 20                        # recent server task history
document-search ocr-tools status                           # Japanese OCR engine installs
document-search ocr-tools install [--reinstall]            # install them ahead of time
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
