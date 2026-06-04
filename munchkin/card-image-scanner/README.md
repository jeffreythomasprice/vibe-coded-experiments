# card-image-scanner

Segments scanned Munchkin card sheets into individual cards, OCRs the text,
extracts the center illustration, and writes everything to a TOML database plus
PNG files.

## Requirements

- Rust (edition 2024) / `cargo`
- The `tesseract` CLI on `PATH` (the tool shells out to it). On Debian/Ubuntu:
  `sudo apt install tesseract-ocr`.
- For the optional illustration vectorizer (`vectorize-illustrations.sh`): the
  `vtracer` CLI on `PATH` (`cargo install vtracer`).

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

## Vectorizing illustrations

`vectorize-illustrations.sh` is an optional post-processing step that converts the
extracted `*_illustration.png` crops into SVG vector art with
[`vtracer`](https://github.com/visioncortex/vtracer). The flat, bold-outlined
Munchkin art vectorizes cleanly, and the result scales to any size without
pixelation.

Install `vtracer` once (`cargo install vtracer`), then run:

```sh
./vectorize-illustrations.sh [IN_DIR] [OUT_DIR]
```

Defaults are `IN_DIR=./out` and `OUT_DIR=./out/svg`. The script finds every
`*_illustration.png` in `IN_DIR`, vectorizes them in parallel (one job per core),
and writes a matching `<name>.svg` into `OUT_DIR`.

### Tweaking the output

The defaults are tuned for these cards: they flatten the parchment background
(which otherwise explodes into hundreds of tiny paths) while keeping the
character art faithful — roughly 80 paths/card instead of 750. Override any
parameter by setting an env var on the command line:

```sh
# keep more fine detail (noisier background, larger files)
FILTER_SPECKLE=6 COLOR_PRECISION=6 ./vectorize-illustrations.sh

# angular, low-poly look instead of smooth curves
MODE=polygon ./vectorize-illustrations.sh
```

| Env var | Default | Effect |
|---------|---------|--------|
| `COLOR_PRECISION` | `5` | Bits of color per channel. Higher = more colors / closer to the original shading; lower = flatter. |
| `FILTER_SPECKLE` | `12` | Discard blobs smaller than N px. Higher = less scan noise but loses fine detail; lower = keeps detail. |
| `GRADIENT_STEP` | `24` | Color difference between stacked gradient layers. Higher = fewer, flatter color bands. |
| `CORNER_THRESHOLD` | `60` | Min angle (deg) treated as a sharp corner vs. a smooth curve. |
| `MODE` | `spline` | `spline` (smooth curves), `polygon` (straight edges), or `pixel`. |
| `JOBS` | `nproc` | Number of parallel `vtracer` workers. |

To preview a result without a dedicated SVG rasterizer, open the `.svg` in a
browser, or screenshot it with headless Chrome:

```sh
google-chrome --headless --screenshot=/tmp/preview.png \
  --window-size=W,H "file://$PWD/out/svg/<name>.svg"
```

### Making the background transparent

vtracer traces every color region, including the card's parchment background, so
its SVGs are opaque. `make-illustrations-transparent.py` removes that background
so you can composite the character art over your own backdrop.

The background can't be picked out by path order or fill color: vtracer splits
the parchment across many paths in slightly different shades, scattered through
the z-order (some under the art, some over it), and some cards paint it on top of
a solid full-canvas base. The only reliable signal is **edge-connectivity** — the
background is whatever is reachable from the canvas border without crossing into
the art. So the script works on *pixels*, not paths:

1. Render the SVG to a bitmap.
2. **Flood-fill inward from the four borders**, growing through pixels within a
   small per-step color tolerance (and staying within a looser tolerance of the
   sampled border color, so the fill can't leap an edge into the art). That
   region is the background.
3. Invert to a foreground mask; fill interior holes, close, and erode 1px to drop
   the anti-aliased fringe.
4. Trace the foreground outline into vector contours (marching squares) and add
   them as a `<clipPath>`, wrapping the original (untouched) vtracer paths in a
   `<g clip-path=...>`. The art stays fully vector; only a clip outline is added.

This removes all background regardless of how many paths or shades drew it, and
handles dark-base cards correctly — the dark base only survives where it's
*inside* the foreground silhouette (the actual character).

The script declares its dependencies inline (PEP 723) and is run with **uv**,
which fetches them into an ephemeral environment — nothing is installed system
wide:

```sh
# dry run — lists what would change, touches nothing
uv run make-illustrations-transparent.py out/svg

# write the changes in place
uv run make-illustrations-transparent.py out/svg --apply
```

`DIR` defaults to `./out/svg`. The operation is idempotent — a processed file
carries a `<clipPath id="bg-clip">` marker and is skipped on re-runs — so it's
safe to re-run, and since the SVGs are tracked in git any result is recoverable
with `git checkout`. Tunables (env vars): `NEIGHBOR_TOL` (per-step fill tolerance,
default 30), `SEED_TOL` (max distance from the border color, default 85),
`ERODE_PX` (fringe trim, default 1), `SIMPLIFY_TOL` (contour simplification px,
default 1.5), `MIN_BG_PCT` (skip near-background-less files, default 1.0).

## Known omissions

- **Convenient Addition Error** — this card is absent from our data set. It was a
  promotional card that isn't included in every version of the game, so it never
  appeared in the scans. We're fine without it; it's documented here only so its
  absence doesn't read as a scanning bug.

## Accuracy notes

Card segmentation and the body/corner text are reliable. The stylized small-caps
title font is the hard part for Tesseract; the card-list fuzzy match recovers the
large majority of titles, and the raw OCR + saved card image let you fix the rest
by hand. Re-run with adjusted flags (no recompile needed) and inspect the
`--debug` overlays to tune.
