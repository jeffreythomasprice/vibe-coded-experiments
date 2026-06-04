# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this repo is

A pipeline + dataset for digitizing the base **Munchkin** card game. The end goal
(see `TODO.md`) is a structured, hand-verified database of every card plus
vector art, suitable for a future card renderer. The repo has two halves:

- `assets/` — the **data**. Raw scans go in; cleaned, structured card data comes
  out. `assets/processed/cards.toml` is the curated source of truth.
- `card-image-scanner/` — the **tool** (Rust CLI) that segments scanned card
  sheets, OCRs them, extracts illustrations, and emits the initial TOML + PNGs.

The flow is one-directional: `assets/raw/*` → scanner → `assets/processed/*`.
Most ongoing work is hand-correcting the data in `assets/processed/cards.toml`,
not re-running the scanner.

## Two schemas — do not confuse them

The scanner emits a **raw OCR-pipeline schema**; the committed dataset uses a
**curated schema**. They are not the same, and the difference is the single
easiest thing to get wrong here.

- **Scanner output schema** — defined in `card-image-scanner/src/model.rs`.
  Includes OCR bookkeeping: `raw_line`, `title_raw`, `title_match_score`,
  `bbox`, `top_extras`, `index_in_sheet`, `row`, `col`. Documented in
  `card-image-scanner/README.md` and `card-image-scanner/CLAUDE.md`.
- **Curated dataset schema** — what lives in `assets/processed/cards.toml`.
  Documented authoritatively in **`assets/processed/README.md`**. Bookkeeping
  fields are dropped; `top_extras` → `below_title`; monster levels lifted into
  `above_title`; image paths point into `images/raw/` and `images/svg/`.

The one-shot `card-image-scanner/migrate_cards_toml.py` is what converted the
raw schema to the curated one. **When editing card data, follow
`assets/processed/README.md`, not `model.rs`.** When changing the scanner's
output, follow `card-image-scanner/CLAUDE.md`.

## Working on the card data (most common task)

Edit `assets/processed/cards.toml` directly. It is hand-curated TOML — preserve
the multi-line block style of `body`/`below_title` arrays. Field semantics,
which fields are optional, plain-string-vs-array rendering, and the per-card-type
meaning of `bottom_left`/`bottom_right` are all spelled out in
`assets/processed/README.md`. Read it before touching the data.

Open TODO items (`TODO.md`) are all data-quality passes: verifying every monster
has a level + treasures, making duplicate cards (e.g. all 3 Wizards) consistent,
and adding markdown to text fields.

## Working on the scanner

`card-image-scanner/CLAUDE.md` is the detailed guide; read it first. Quick
reference:

```sh
cd card-image-scanner
cargo build --release
cargo run --release -- '../assets/raw/rules-and-card-scans/door*.jpg' \
                       '../assets/raw/rules-and-card-scans/loot*.jpg' \
                       --out-dir ./out
cargo run --release -- <IMAGES...> --debug   # write annotated overlays for tuning
```

Requires the `tesseract` CLI on `PATH`. Parameters are runtime flags (no
recompile needed to retune); use `--debug` overlays to tune segmentation/layout.

### Illustration vectorization (separate, optional stages)

Two standalone post-processing steps the Rust binary does **not** call — they
only communicate via the `*_illustration.png` filenames on disk:

```sh
./vectorize-illustrations.sh [IN_DIR] [OUT_DIR]   # needs `vtracer` (cargo install vtracer)
uv run make-illustrations-transparent.py [DIR] --apply   # PEP 723 / uv; clips out the background
```

Both are tuned via env vars and documented in `card-image-scanner/README.md` and
`CLAUDE.md`. The transparency script is idempotent (marks processed files with a
`<clipPath id="bg-clip">`).

## Keeping docs in sync

This codebase relies heavily on prose docs as the spec. If you change:
- the curated card schema → update `assets/processed/README.md`.
- the scanner TOML shape → update `card-image-scanner/src/model.rs` +
  `card-image-scanner/README.md` + `card-image-scanner/CLAUDE.md`.
- the vectorize/transparency tunables → update the script **and** its header
  comment **and** the README env-var table (they have drifted before).
