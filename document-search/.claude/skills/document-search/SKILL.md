---
name: document-search
description: Answer questions from locally-ingested documents using the `document-search` CLI (vector search over Ollama embeddings + turso). Use when the user asks a factual question that should be answered from their own documents — e.g. "what does the rulebook say about X", "find the section on Y in <doc>", "search my docs for Z" — wants an overview or summary of an ingested document, wants to read specific pages, asks what's ingested or what state a document is in, or explicitly invokes document-search. Assumes documents are already ingested; this skill does not ingest.
---

# document-search

Use the `document-search` binary to ground answers in the user's local document corpus. Workflow for a question: pick scope → run vector search → read hits → answer with citations. The [command reference](#command-reference) below covers the other read-only lookups (metadata, pages, summaries, job state).

Always pass `--output-mode json` and parse the result. Every JSON reply has `ok: true`; on error the CLI prints a human-readable message and exits non-zero.

## 1. Pick the scope

`search` requires exactly one of `--path <exact path>` or `--tag <tag>` (repeatable, with optional `--match-all`). If the conversation already names a document or tag, use it. Otherwise discover what's available:

```bash
document-search list --output-mode json
document-search tag list --output-mode json
```

Decision rules:
- Exactly one ingested document and no tags relevant → use `--path` with that document, no need to ask.
- Multiple documents and the question clearly matches one title → use that `--path`, but mention the choice in your response so the user can redirect.
- Tags exist and the question matches a tag's topic → use `--tag`.
- Ambiguous → ask the user which path or tag(s) to scope to. List the available options so they can pick.

## 2. Run the search

Always use JSON output and full chunk text. Vector search is sensitive to phrasing — restate the user's question as a short declarative noun phrase or assertion rather than passing the literal question.

```bash
document-search search "<rephrased term>" \
  --path "<exact path>" \
  --output-mode json \
  --no-truncate
```

Or with tags:

```bash
document-search search "<rephrased term>" \
  --tag "<tag>" [--tag "<tag2>" [--match-all]] \
  --output-mode json --no-truncate
```

Useful flags:
- `--limit <N>` — chunks returned per in-scope document. Bump to 5–10 when the question is broad or the top hits are thin.
- `--cutoff <0.0–1.0>` — drop chunks below this similarity. Default comes from config; lower it (e.g. `0.2`) to see weaker hits, raise it (e.g. `0.5`) to keep only strong ones.
- `--include-summaries` — also vector-search the per-document summary tree. Only works if the doc has summaries (`info` shows them); safe to try and fall back if it errors.

## 3. Read the results

JSON shape: `{ ok, term, cutoff, limit, results: [{ path, similarity, page_first, page_last, byte_start, byte_end, snippet, truncated }] }`.

With `--include-summaries` the results are grouped by document instead: `{ ok, term, cutoff, limit, include_summaries: true, documents: [{ path, region_summary, region_summary_level, region_summary_language, hits: [{ byte_start, byte_end, page_first, page_last, similarity, snippet, truncated, source }] }] }`. `region_summary_language` is an ISO 639-1 code (`en`, `ja`, …); documents may have summaries in several languages and the preferred one is picked when available.

Embeddings are multilingual, so an English query can match chunks of a non-English document (and vice versa). Snippets come back in the document's own language — translate when answering if needed, and consider re-searching in the document's language when cross-language hits are weak.

Heuristics:
- `similarity` ≥ 0.6 — strong, likely directly relevant.
- 0.4–0.6 — plausibly relevant; read carefully before citing.
- < 0.4 — weak; usually not enough to answer on its own. Try a rephrasing or a broader scope before giving up.

If the top hits are weak or contradictory, re-search with a different phrasing (synonyms, the key noun, a related claim). Two or three rephrasings is reasonable; don't loop forever.

If a snippet cuts off mid-thought, pull the surrounding pages with `text --pages` (see [Reading text](#reading-text)), using the hit's `page_first`/`page_last` widened by ±1 page, or `byte_start`/`byte_end` for `.txt` documents.

## 4. Overviews: read the summary tree instead of searching

When the user wants a summary or overview of a document (or part of one) rather than a specific fact, read the prebuilt summary tree (see [Summary tree](#summary-tree)): `--outline` first, then the top level, then drill down through `children` only where the user wants more detail. Summaries are LLM-written and can garble names or details, so check against `text --pages` before stating something specific. If it errors with "has no summaries", the document was ingested with `--no-summary`; fall back to search and `text`.

## 5. Answer

Compose a direct answer to the user's question, grounded in the chunks. For each claim, cite the source as `path:p<page>` (PDFs) or `path` plus a byte/char range. Quote sparingly — prefer paraphrase with a short verbatim phrase when wording matters.

If the search came up empty or only weak hits across several rephrasings, say so plainly rather than inventing an answer. Suggest the user verify the doc is ingested (`document-search list`) or try a different scope.

## Command reference

All of these are read-only. `list`, `info`, `summaries`, `tag list`, `status`, `queue list`, and `task-log` answer immediately even while a long ingest is running; `search` and `text` wait behind any queued job.

### What's ingested

```bash
# Every document (plus any running/queued job).
document-search list --output-mode json
# Only documents with a tag; --match-all requires every tag instead of any.
document-search list --tag rules --tag 2e --match-all --output-mode json
# Tags with usage counts.
document-search tag list --output-mode json
```

`list` → `{ ok, documents: [{ path, type, total_size_bytes, total_size_pages, created_at, tags }], current: { id, label, running_secs } | null, queued: [{ id, label, queued_secs }] }`. `type` is `pdf` or `txt`; `total_size_pages` is null for `.txt`.

`tag list` → `{ ok, tags: [{ tag, count }] }`.

### Document metadata

```bash
document-search info --output-mode json "<path>"
```

→ `{ ok, path, ingested, type, total_size_bytes, total_size_chars, total_size_pages, language, summaries, tags, pending: [{ id, state, position, effect, label }] }`.

- `language` — ISO 639-1 code detected at ingest, or null.
- `summaries` — `{ "<lang>": <row count> }`, one key per summary tree; `{}` means no summaries. Use it to pick `summaries --lang` and to decide whether `--include-summaries` will work.
- `total_size_pages` — the valid range for `text --pages` / `summaries --pages` is `1..=total_size_pages`.
- `pending` — queued or running jobs that will `create`, `update`, or `delete` this document. A path that isn't ingested yet still succeeds (`ingested: false`) while a job targets it; tell the user it's still being ingested rather than saying it doesn't exist.

### Reading text

Pick exactly one of `--pages`, `--chars`, `--bytes`. Ranges are inclusive on both ends.

```bash
# One page (PDFs only; 1-based).
document-search text --pages 12 12 --output-mode json "<path>"
# A page range, e.g. a whole chapter found via search hits or the summary outline.
document-search text --pages 12 18 --output-mode json "<path>"
# The last page: get total_size_pages from `info` first.
document-search text --pages <N> <N> --output-mode json "<path>"
# Character or byte ranges (any doc type; use these for .txt).
document-search text --chars 0 1999 --output-mode json "<path>"
document-search text --bytes <byte_start> <byte_end> --output-mode json "<path>"
```

→ `{ ok, path, range: { type: "pages", first, last } | { type: "chars"|"bytes", start, end }, text }`. Pages inside a multi-page range are separated by a form feed (`\f`, U+000C), so you can split `text` on it to recover per-page text — page `first + i` is element `i`.

To get a whole document rather than a range, `export` prints it (untranslated and unedited with `--no-edit`; no file output, so it only reads and answers immediately):

```bash
document-search export --no-edit --output-mode json "<path>"                  # txt
document-search export --no-edit --output-md - --output-mode json "<path>"    # markdown, headings guessed
```

→ `{ ok, path, format, source_language, language, translated: false, translation_skipped: false, edited: false, edit_segments_kept: 0, intermediates: null, outputs: [{ format, path: null, bytes }], text }`. A large document floods the context just like a large `text` range, so prefer `summaries` or a page range unless the user wants the whole thing. Always pass `--no-edit`: without it the export runs an LLM grammar-edit pass as a long queued job. Don't use `--to` (LLM translation, also a long queued job) or `--output-*` file paths unless the user asks for a file.

The text is what was extracted at ingest (pdftotext, or OCR for scanned/garbled pages), not the original layout: tables and figures come out as fragments, and OCR'd pages can contain misreads. Read a few pages at a time rather than a whole book — large ranges flood the context.

### Summary tree

```bash
# Shape: levels, counts, and each summary's pages / bytes / parent / children, without text.
document-search summaries --outline --output-mode json "<path>"
# The root (use the highest level from `levels`) — a whole-document overview.
document-search summaries --level <top> --output-mode json "<path>"
# Every summary at one level, or a slice of it by 0-based index.
document-search summaries --level 1 --output-mode json "<path>"
document-search summaries --level 0 --index 4 7 --output-mode json "<path>"
# Every summary (all levels) whose span overlaps a page or byte range.
document-search summaries --pages 30 45 --output-mode json "<path>"
document-search summaries --bytes <start> <end> --output-mode json "<path>"
# A specific language tree, when `info` shows several.
document-search summaries --lang ja --level 0 --output-mode json "<path>"
```

→ `{ ok, path, language, available_languages, levels: [{ level, count }], summaries: [{ id, level, index, parent_index, children: { first, last } | null, page_first, page_last, byte_start, byte_end, text }] }`. `text` is omitted with `--outline`; `byte_end` is exclusive. Level 0 is the leaf level (each covers a group of chunks); each higher level summarizes the one below, and `children` is the index range in the level below. `--index` requires `--level`; `--index`, `--pages`, and `--bytes` are mutually exclusive. Combine `--outline` with filters to outline just part of the tree. Without `--lang`, the only tree is used, else the configured preferred language.

Patterns:
- "Summarize this document" → `--outline` to find the top level, then `--level <top>`. If that's too terse, read the level below.
- "What happens in pages X–Y / chapter Z" → `--pages X Y` (overlap), then confirm details with `text --pages`.
- "Give me a page-by-page / section-by-section rundown" → `--level 0`; each entry's `page_first`/`page_last` gives the citation.

### Server and job state

Use these when a document is missing from `list`, `info` shows `pending` jobs, or the user asks what's running.

```bash
# Running job with progress/ETA, plus the queue.
document-search status --output-mode json
document-search queue list --output-mode json
# Recent jobs, oldest first.
document-search task-log -n 20 --output-mode json
```

`task-log` → `{ ok, tasks: [{ job_id, task_name, label, path, tags, started_at, ended_at, duration_secs, status, error }] }`. `status` is `in-progress`, `success`, `partial`, or `failure` (a cancel is a `failure` with `error: "cancelled"`).

A `partial` ingest in `task-log` means the document and its chunks were stored but the summary phase failed — search and `text` work, `summaries` may be missing or incomplete.

## Notes

- `search` errors with "must specify --path or at least one --tag" if scope is omitted — never call it bare.
- Paths must be the **exact** ingested path, not a substring. Get it from `list` output. Quote paths — they often contain spaces and parentheses.
- Tags are lowercased and trimmed by the CLI.
- This skill does not ingest or modify anything. Don't run `ingest`, `summarize`, `delete`, `tag add`/`tag remove`, or `queue delete`/`queue clear`/`queue cleanup` unless the user explicitly asks. If the user asks about a document that isn't in `list` (and has no pending job in `info`), tell them and stop.
