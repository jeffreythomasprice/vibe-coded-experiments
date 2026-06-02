#!/usr/bin/env bash
# Vectorize the extracted Munchkin card illustrations to SVG using vtracer.
#
# Requires: vtracer on PATH  (cargo install vtracer)
#
# Usage:
#   ./vectorize-illustrations.sh [IN_DIR] [OUT_DIR]
#
# Defaults: IN_DIR=./out  OUT_DIR=./out/svg
#
# Tuning (override via env vars):
#   COLOR_PRECISION  bits of color per channel; lower = fewer, flatter colors (default 6)
#   FILTER_SPECKLE   discard blobs smaller than N px; higher = less scan noise (default 6)
#   GRADIENT_STEP    color difference between gradient layers (default 16)
#   CORNER_THRESHOLD min angle (deg) to treat a corner as sharp vs. smooth (default 60)
#   MODE             spline | polygon | pixel  (default spline)
#   JOBS             parallel workers (default = nproc)

set -euo pipefail

IN_DIR="${1:-./out}"
OUT_DIR="${2:-./out/svg}"

# Defaults tuned for Munchkin illustrations: flat parchment background, faithful
# character art. Lower FILTER_SPECKLE (e.g. 4-6) to keep more fine detail at the
# cost of a noisier background and bigger files.
COLOR_PRECISION="${COLOR_PRECISION:-5}"
FILTER_SPECKLE="${FILTER_SPECKLE:-12}"
GRADIENT_STEP="${GRADIENT_STEP:-24}"
CORNER_THRESHOLD="${CORNER_THRESHOLD:-60}"
MODE="${MODE:-spline}"
JOBS="${JOBS:-$(nproc)}"

if ! command -v vtracer >/dev/null 2>&1; then
  echo "error: vtracer not found on PATH. Install it with: cargo install vtracer" >&2
  exit 1
fi

mkdir -p "$OUT_DIR"

vectorize_one() {
  in="$1"
  base="$(basename "${in%.png}")"
  out="$OUT_DIR/$base.svg"
  vtracer \
    --input "$in" \
    --output "$out" \
    --mode "$MODE" \
    --color_precision "$COLOR_PRECISION" \
    --filter_speckle "$FILTER_SPECKLE" \
    --gradient_step "$GRADIENT_STEP" \
    --corner_threshold "$CORNER_THRESHOLD" \
    && echo "ok  $base.svg"
}
export -f vectorize_one
export OUT_DIR MODE COLOR_PRECISION FILTER_SPECKLE GRADIENT_STEP CORNER_THRESHOLD

count="$(find "$IN_DIR" -maxdepth 1 -name '*_illustration.png' | wc -l)"
echo "Vectorizing $count illustrations from $IN_DIR -> $OUT_DIR (jobs=$JOBS, mode=$MODE)"

find "$IN_DIR" -maxdepth 1 -name '*_illustration.png' -print0 \
  | xargs -0 -P "$JOBS" -I{} bash -c 'vectorize_one "$@"' _ {}

echo "Done. SVGs in $OUT_DIR"
