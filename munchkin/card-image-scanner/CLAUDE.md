# card-image-scanner — agent notes

Rust (edition 2024) CLI that segments scanned Munchkin card sheets into
individual cards, OCRs them, extracts the center illustration, and emits a TOML
database plus PNGs. See `README.md` for the user-facing usage and the full flag
table; this file documents the two output pipelines an agent is most likely to
touch and to keep straight: **TOML generation** (built into the Rust binary) and
**SVG generation** (a separate, optional shell script).

These are two distinct, independent stages. The Rust binary never produces SVGs;
`vectorize-illustrations.sh` never touches the TOML. They communicate only
through the PNG files on disk.

## Stage 1 — TOML generation (Rust binary)

The TOML database is the primary output. It's produced entirely in-process by
the `cargo run` binary; no external tool is involved except `tesseract` (for OCR
of the text that fills the TOML fields).

### Where it lives in the code

- `src/model.rs` — the serde data model. This is the **source of truth for the
  TOML shape**. `Database` is the top-level table; its `card_list: Vec<Card>`
  field is `#[serde(rename = "card")]`, so it serializes as a `[[card]]`
  array-of-tables. `Card`, `RawLine`, and `BBox` are the nested structs.
- `src/main.rs` — `run()` builds the `Database`, pushes one `Card` per detected
  card, then serializes with `toml::to_string_pretty(&db)` and writes it to
  `db_path` (lines ~63 and ~195). `db_path` defaults to `<out-dir>/cards.toml`,
  overridable with `--db`.
- The `timestamp()` helper writes the `generated` field as `epoch:<secs>` — a
  deliberately crate-free coarse timestamp, **not** RFC3339 despite the doc
  comment in `model.rs`. If you change this, update both places.

### Editing the TOML schema — rules

- To add/remove/rename a field, edit the struct in `src/model.rs`. The TOML key
  follows the Rust field name unless overridden with `#[serde(rename = ...)]`.
- Optional fields use `#[serde(skip_serializing_if = ...)]` so absent data
  doesn't emit empty keys: `Option::is_none` for scalars, `Vec::is_empty` for
  lists. Preserve this pattern — it keeps the TOML clean and is relied on by
  anything parsing the output.
- `raw_lines` is renamed to `raw_line` (singular) so each line becomes a
  `[[card.raw_line]]` sub-table. The `raw_line` array is the **lossless** record
  of every OCR line; never drop it for brevity — the structured fields (`title`,
  `body`, corners) are best-effort and `raw_line` is the fallback when they're
  wrong.
- The model derives `Serialize` only (it's write-only). There is no
  deserialization path in this crate, so don't assume round-tripping.
- Field order in the struct = key order in the emitted TOML. Keep related fields
  grouped (ids, then text, then asset paths, then raw lines) as they are now.

### What populates the fields

`title`/`title_raw`/`title_match_score` come from `correct_title()` in
`main.rs` (fuzzy match against `Munchkin-CardList.txt` via `cardlist.rs`).
`top_extras`/`body`/`bottom_left`/`bottom_right`/`illustration` come from
`layout::analyze` (`layout.rs`). `raw_lines` come straight from `ocr.rs`. The
`*_card.png` and `*_illustration.png` asset paths stored in the TOML are
**relative to the db file**, not absolute.

## Stage 2 — SVG generation (separate shell script)

`vectorize-illustrations.sh` is a standalone, optional post-processing step. It
is **not** invoked by the Rust binary and has no Rust code — it's a bash wrapper
around the external `vtracer` CLI (`cargo install vtracer`).

### What it does

- Reads `*_illustration.png` crops (the ones Stage 1 wrote), and for each one
  runs `vtracer` to produce a matching `<name>.svg`.
- `IN_DIR` defaults to `./out`, `OUT_DIR` to `./out/svg` (positional args
  `$1`/`$2`).
- Runs in parallel via `xargs -P "$JOBS"` (default `JOBS=$(nproc)`), one
  `vtracer` invocation per illustration through the `vectorize_one` bash
  function (which is `export -f`'d along with its env-var parameters so the
  subshells can see them).
- Hard-fails early if `vtracer` isn't on `PATH`.

### Tunables (env vars)

All tuning is via env vars read with `${VAR:-default}` at the top of the script;
there are no flags. The defaults are tuned for Munchkin art: flatten the
parchment background while keeping the character art faithful (~80 paths/card vs
~750).

| Env var | Default | Effect |
|---------|---------|--------|
| `COLOR_PRECISION` | `5` | bits of color/channel; lower = flatter |
| `FILTER_SPECKLE` | `12` | discard blobs < N px; higher = less noise, less detail |
| `GRADIENT_STEP` | `24` | color delta between gradient layers |
| `CORNER_THRESHOLD` | `60` | min angle (deg) treated as a sharp corner |
| `MODE` | `spline` | `spline` \| `polygon` \| `pixel` |
| `JOBS` | `nproc` | parallel `vtracer` workers |

Note: the **header comment block** inside the script still lists some stale
defaults (`COLOR_PRECISION 6`, `FILTER_SPECKLE 6`, `GRADIENT_STEP 16`); the
actual `${VAR:-...}` lines are authoritative (5/12/24). If you touch the
defaults, fix the comment too — `README.md` documents the real values and must
also stay in sync.

### Known limitation

`vtracer` traces every color region including the background, so output SVGs are
opaque (no transparency). Isolating the character on a transparent background
would be a separate masking step — not currently implemented.

## When you change either stage

- Touch the TOML shape → edit `src/model.rs`, and update the "Outputs" / schema
  description in `README.md`.
- Touch the SVG tunables/behavior → edit `vectorize-illustrations.sh` (both the
  `${VAR:-...}` lines **and** the header comment) and the SVG section + env-var
  table in `README.md`.
- The two stages share only the `*_illustration.png` filenames. If you rename
  those in `main.rs`, the `find ... -name '*_illustration.png'` glob in the
  script must change to match.
