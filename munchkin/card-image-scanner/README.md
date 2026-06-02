# card-image-scanner

Segments scanned Munchkin card sheets into individual cards, OCRs the text,
extracts the center illustration, and writes everything to a TOML database plus
PNG files.

## Requirements

- Rust (edition 2024) / `cargo`
- The `tesseract` CLI on `PATH` (the tool shells out to it). On Debian/Ubuntu:
  `sudo apt install tesseract-ocr`.

## Usage

```sh
cargo run --release -- <IMAGES...> [OPTIONS]
```

Example (all the scans in this repo):

```sh
cargo run --release -- \
  '../rules-and-card-scans/door*.jpg' \
  '../rules-and-card-scans/loot*.jpg' \
  --out-dir ./out
```

Outputs:

- `out/cards.toml` — one `[[card]]` per card with the structured text fields and
  the relative paths to its images.
- `out/<id>_card.png` — the cropped card.
- `out/<id>_illustration.png` — the cropped center illustration (when found).

Add `--debug` to also write, under `out/debug/`, the binarized sheet, the sheet
with detected card boxes outlined, and per-card overlays showing the detected
illustration band (green), OCR lines (blue), and bottom-corner zones
(orange/purple). These are the fastest way to tune the parameters below.

## How it works

1. **Segment** (`segment.rs`): binarize the sheet (Otsu), close small gaps in
   the dark card borders, then take each border ring as a connected component.
   Boxes are filtered by card aspect (~0.62) and area, de-duplicated by
   containment (so an illustration inside a card isn't counted), and ordered
   row-major with skew tolerance. A single tightly-cropped card (e.g. the card
   backs) is handled by a whole-image fallback. Grid size is detected
   automatically; `--rows/--cols` force a fixed grid and `--segment-mode
   projection` switches to white-gutter profiles.
2. **OCR** (`ocr.rs`): each card crop is OCR'd via `tesseract ... --psm 11 tsv`,
   parsed into text lines (words grouped by Tesseract line) with bounding box,
   glyph height, and confidence.
3. **Layout** (`layout.rs`): the illustration is the tallest run of ink-dense
   rows not occupied by confident text. Lines are then classified into the
   title (tallest in the top third), `top_extras` (smaller header lines),
   `body` (stitched, wrapped lines re-joined), and the two bottom corners.
4. **Title correction** (`cardlist.rs`): the OCR'd title is fuzzy-matched
   (Jaro–Winkler, with sliding token windows) against the known names in
   `Munchkin-CardList.txt`, so "LEVEL 8 FACE SUCKER" → "Face Sucker". Matches at
   or above `--title-match-threshold` (0.82) replace the title; the raw OCR and
   the match score are always kept. Every OCR line is also stored verbatim in
   `raw_line` so nothing is lost when a guess is wrong.

## Key options

| Flag | Default | Purpose |
|------|---------|---------|
| `--out-dir <DIR>` | `./out` | Where PNGs and (by default) the DB go |
| `--db <FILE>` | `<out-dir>/cards.toml` | Aggregate TOML database path |
| `--card-list <FILE>` | next to inputs | `Munchkin-CardList.txt` for title correction |
| `--type door\|loot\|auto` | `auto` | Card type; auto-inferred from filename |
| `--tess-psm <N>` | `11` | Tesseract page-segmentation mode |
| `--min-conf <N>` | `30` | Drop OCR words below this confidence |
| `--title-match-threshold <F>` | `0.82` | Fuzzy-match cutoff for title correction |
| `--bin-threshold <0-255>` | Otsu | Manual binarization threshold |
| `--morph-radius <N>` | `3` | Border-gap closing radius |
| `--aspect-min/--aspect-max` | `0.55`/`0.78` | Card aspect-ratio acceptance band |
| `--rows`/`--cols` | auto | Force a fixed grid |
| `--debug` | off | Write intermediate/annotated images |

## Accuracy notes

Card segmentation and the body/corner text are reliable. The stylized small-caps
title font is the hard part for Tesseract; the card-list fuzzy match recovers the
large majority of titles, and the raw OCR + saved card image let you fix the rest
by hand. Re-run with adjusted flags (no recompile needed) and inspect the
`--debug` overlays to tune.
