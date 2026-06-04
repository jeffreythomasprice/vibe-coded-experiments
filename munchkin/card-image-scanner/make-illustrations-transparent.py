#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["cairosvg", "pillow", "numpy", "scipy", "scikit-image"]
# ///
"""Make vtracer illustration SVGs transparent by flood-filling the background.

vtracer traces every color region, including the card's parchment background. It
does NOT emit the background as one tidy leading shape: the parchment is split
across many paths in slightly different shades, scattered through the z-order
(some painted *under* the art, some *over* it), and some cards paint the
background on top of a solid full-canvas base. So the background can't be
identified by path order or fill color -- the only reliable signal is
**edge-connectivity**: the background is the region reachable from the canvas
border without crossing into the character art.

This script therefore works on *pixels*, not paths:

  1. Render the SVG to a bitmap (cairosvg).
  2. Flood-fill inward from the four borders, growing through pixels within a
     small per-step color tolerance (and staying within a looser tolerance of the
     sampled border color, so it can't leap across an edge into the art). The
     filled region is the background.
  3. Invert -> foreground mask; fill interior holes, close, and erode 1px to drop
     the anti-aliased fringe.
  4. Trace the foreground boundary into vector contours (marching squares) and
     emit them as a `<clipPath>`. Wrap the original (untouched) vtracer paths in
     a `<g clip-path=...>` so everything outside the foreground silhouette is
     clipped away. The art stays fully vector; only a clip outline is added.

This removes ALL background regardless of how many paths or shades drew it, and
handles dark-base cards (the dark base only survives where it's *inside* the
foreground silhouette, i.e. the actual character).

Run it with uv (fetches deps into an ephemeral env; nothing is installed system
wide):

    uv run make-illustrations-transparent.py [DIR] [--apply]

    DIR       directory of *.svg files (default: ./out/svg)
    --apply   write changes in place; without it, this is a dry run

Tuning (env vars):
    NEIGHBOR_TOL   max per-step L1 color delta when growing the fill (default 30)
    SEED_TOL       max L1 distance from the sampled border color a background
                   pixel may have (default 85)
    ERODE_PX       pixels to erode the foreground, trimming the AA fringe (1)
    SIMPLIFY_TOL   contour simplification tolerance in px (default 1.5)
    MIN_BG_PCT     skip files whose background is below this % (default 1.0)

Idempotent: a processed file carries a ``<clipPath id="bg-clip">`` marker and is
skipped on re-runs. Files are tracked in git, so any result is recoverable.
"""
import os
import re
import sys
from collections import deque
from pathlib import Path

import cairosvg
import numpy as np
from PIL import Image
from scipy import ndimage
from skimage import measure

WH_RE = re.compile(r'<svg\b[^>]*\bwidth="(\d+)"[^>]*\bheight="(\d+)"')
SVG_OPEN_RE = re.compile(r"<svg\b[^>]*?>", re.DOTALL)
MARKER = 'id="bg-clip"'

NEIGHBOR_TOL = int(os.environ.get("NEIGHBOR_TOL", "30"))
SEED_TOL = int(os.environ.get("SEED_TOL", "85"))
ERODE_PX = int(os.environ.get("ERODE_PX", "1"))
SIMPLIFY_TOL = float(os.environ.get("SIMPLIFY_TOL", "1.5"))
MIN_BG_PCT = float(os.environ.get("MIN_BG_PCT", "1.0"))


def render_rgb(svg_text: str, w: int, h: int) -> np.ndarray:
    """Render an SVG string to an (h, w, 3) int16 RGB array."""
    cairosvg.svg2png(bytestring=svg_text.encode(), write_to="/tmp/_transp_render.png",
                     output_width=w, output_height=h)
    im = Image.open("/tmp/_transp_render.png").convert("RGB")
    return np.asarray(im, dtype=np.int16)


def background_mask(arr: np.ndarray) -> np.ndarray:
    """Flood-fill the edge-connected background; return a bool (h, w) mask."""
    h, w = arr.shape[:2]
    border = np.concatenate([arr[0, :], arr[-1, :], arr[:, 0], arr[:, -1]]).reshape(-1, 3)
    seed = np.median(border, axis=0)
    far_from_seed = np.abs(arr - seed).sum(axis=2) > SEED_TOL

    visited = np.zeros((h, w), dtype=bool)
    dq: deque[tuple[int, int]] = deque()
    for x in range(w):
        dq.append((0, x))
        dq.append((h - 1, x))
    for y in range(h):
        dq.append((y, 0))
        dq.append((y, w - 1))

    while dq:
        y, x = dq.popleft()
        if visited[y, x] or far_from_seed[y, x]:
            continue
        visited[y, x] = True
        c0 = arr[y, x]
        for dy, dx in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            ny, nx = y + dy, x + dx
            if (0 <= ny < h and 0 <= nx < w and not visited[ny, nx]
                    and np.abs(arr[ny, nx] - c0).sum() <= NEIGHBOR_TOL):
                dq.append((ny, nx))
    return visited


def foreground_contours(fg: np.ndarray) -> list[str]:
    """Trace the foreground mask boundary into SVG path `d` strings."""
    padded = np.pad(fg.astype(float), 1)  # pad so edge-touching shapes close cleanly
    paths = []
    for contour in measure.find_contours(padded, 0.5):
        poly = measure.approximate_polygon(contour, tolerance=SIMPLIFY_TOL)
        if len(poly) < 3:
            continue
        # contour rows/cols are (y, x); undo the 1px pad
        pts = [f"{x - 1:.1f} {y - 1:.1f}" for y, x in poly]
        paths.append("M" + " L".join(pts) + " Z")
    return paths


def clip_svg(text: str, clip_paths: list[str]) -> str:
    """Wrap the SVG's contents in a clip-path built from clip_paths."""
    m = SVG_OPEN_RE.search(text)
    open_end = m.end()
    defs = ('<defs><clipPath id="bg-clip" clipPathUnits="userSpaceOnUse">'
            + "".join(f'<path d="{d}"/>' for d in clip_paths)
            + "</clipPath></defs>"
            '<g clip-path="url(#bg-clip)">')
    close_at = text.rindex("</svg>")
    return text[:open_end] + defs + text[open_end:close_at] + "</g>" + text[close_at:]


def process(text: str) -> tuple[str | None, float, str]:
    """Return (new_text|None, bg_pct, note). new_text is None when unchanged."""
    if MARKER in text:
        return None, 0.0, "already processed"
    wh = WH_RE.search(text)
    if not wh:
        return None, 0.0, "no width/height"
    w, h = int(wh.group(1)), int(wh.group(2))

    arr = render_rgb(text, w, h)
    bg = background_mask(arr)
    bg_pct = 100.0 * bg.mean()
    if bg_pct < MIN_BG_PCT:
        return None, bg_pct, "no background detected"

    fg = ndimage.binary_fill_holes(~bg)
    fg = ndimage.binary_closing(fg, iterations=2)
    if ERODE_PX:
        fg = ndimage.binary_erosion(fg, iterations=ERODE_PX)
    if not fg.any():
        return None, bg_pct, "empty foreground"

    contours = foreground_contours(fg)
    if not contours:
        return None, bg_pct, "no contours"
    return clip_svg(text, contours), bg_pct, f"{len(contours)} contour(s)"


def main() -> int:
    args = [a for a in sys.argv[1:] if a != "--apply"]
    apply = "--apply" in sys.argv
    svg_dir = Path(args[0]) if args else Path("./out/svg")

    files = sorted(svg_dir.glob("*.svg"))
    if not files:
        print(f"No .svg files found in {svg_dir}", file=sys.stderr)
        return 1

    changed = skipped = 0
    for f in files:
        text = f.read_text()
        new, bg_pct, note = process(text)
        if new is None:
            skipped += 1
            continue
        changed += 1
        print(f"{'OK  ' if apply else 'DRY '} {f.name}: {bg_pct:.0f}% background -> clipped ({note})")
        if apply:
            f.write_text(new)

    verb = "clipped" if apply else "would clip"
    print(f"\n{verb} {changed}/{len(files)} files; skipped {skipped}"
          + ("" if apply else "  (dry run; pass --apply to write)"))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
