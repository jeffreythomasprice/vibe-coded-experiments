---
name: document-search
description: Answer questions from locally-ingested documents using the `document-search` CLI (vector search over Ollama embeddings + turso). Use when the user asks a factual question that should be answered from their own documents — e.g. "what does the rulebook say about X", "find the section on Y in <doc>", "search my docs for Z" — or explicitly invokes document-search. Assumes documents are already ingested; this skill does not ingest.
---

# document-search

Use the `document-search` binary to ground answers in the user's local document corpus. Workflow: pick scope → run vector search → read hits → answer with citations.

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
- `--include-summaries` — also vector-search the per-document summary tree. Only works if `summarize` has been run for the doc; safe to try and fall back if it errors.

## 3. Read the results

JSON shape: `{ ok, term, cutoff, limit, results: [{ path, similarity, page_first, page_last, byte_start, byte_end, snippet, truncated }] }`.

Heuristics:
- `similarity` ≥ 0.6 — strong, likely directly relevant.
- 0.4–0.6 — plausibly relevant; read carefully before citing.
- < 0.4 — weak; usually not enough to answer on its own. Try a rephrasing or a broader scope before giving up.

If the top hits are weak or contradictory, re-search with a different phrasing (synonyms, the key noun, a related claim). Two or three rephrasings is reasonable; don't loop forever.

## 4. Get more context when a snippet is incomplete

If a snippet looks like it cuts off mid-thought and the answer needs more, pull the surrounding region:

```bash
# PDFs: pages
document-search text --pages <first> <last> "<path>"

# Any doc: chars or bytes
document-search text --chars <start> <end> "<path>"
document-search text --bytes <start> <end> "<path>"
```

Use the `page_first`/`page_last` or `byte_start`/`byte_end` from the hit, optionally widened by ±1 page.

## 5. Answer

Compose a direct answer to the user's question, grounded in the chunks. For each claim, cite the source as `path:p<page>` (PDFs) or `path` plus a byte/char range. Quote sparingly — prefer paraphrase with a short verbatim phrase when wording matters.

If the search came up empty or only weak hits across several rephrasings, say so plainly rather than inventing an answer. Suggest the user verify the doc is ingested (`document-search list`) or try a different scope.

## Notes

- `search` errors with "must specify --path or at least one --tag" if scope is omitted — never call it bare.
- `--path` must be the **exact** ingested path string, not a substring. Get it from `list` output.
- Tags are lowercased and trimmed by the CLI.
- This skill does not ingest documents. If the user asks about a document that isn't in `list`, tell them and stop — don't try to ingest it without being asked.
