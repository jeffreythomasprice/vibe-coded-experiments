#!/usr/bin/env python3
"""Strip the solid background out of vtracer-generated illustration SVGs.

vtracer traces every color region, including the card's parchment background,
which it emits as the *first* <path> in the file: a shape anchored at the origin
(``d="M0 0 ..."``) whose extent spans the whole canvas. Some cards have a
two-tone background, so vtracer stacks two such full-canvas paths at the front.

Removing those leading full-canvas paths leaves the character art on a
transparent background (SVG has no default backdrop), ready to composite over
whatever your own renderer draws behind it.

Detection is deliberately conservative: a path is treated as background only if
it *both* starts at ``M0 0`` *and* its coordinates reach the canvas edges. A
foreground element that merely touches an edge, or that starts somewhere other
than the origin, is left alone. At least one path is always kept.

Usage:
    ./make-illustrations-transparent.py [DIR] [--apply]

    DIR       directory of *.svg files (default: ./out/svg)
    --apply   write changes in place; without it, this is a dry run

The operation is idempotent: a second run finds no full-canvas paths and is a
no-op. Files are tracked in git, so an over-eager removal is recoverable.
"""
import re
import sys
from pathlib import Path

WH_RE = re.compile(r'<svg\b[^>]*\bwidth="(\d+)"[^>]*\bheight="(\d+)"')
PATH_RE = re.compile(r'[ \t]*<path\b[^>]*?/>\n?', re.DOTALL)
D_RE = re.compile(r'\bd="([^"]*)"')
NUM_RE = re.compile(r'-?\d+(?:\.\d+)?')

# Safety cap: never strip more than this many leading paths, in case a card is
# (wrongly) traced as nothing but stacked full-canvas shapes.
MAX_LAYERS = 4


def is_full_canvas(d: str, w: int, h: int) -> bool:
    """True if path d starts at the origin and spans the entire w x h canvas."""
    if not d.lstrip().startswith("M0 0"):
        return False
    nums = [float(x) for x in NUM_RE.findall(d)]
    if len(nums) < 2:
        return False
    xs, ys = nums[0::2], nums[1::2]
    return max(xs) >= w - 1 and max(ys) >= h - 1


def strip_background(text: str) -> tuple[str, int]:
    """Return (new_text, layers_removed) after dropping leading full-canvas paths."""
    wh = WH_RE.search(text)
    if not wh:
        return text, 0
    w, h = int(wh.group(1)), int(wh.group(2))

    removed = 0
    while removed < MAX_LAYERS:
        m = PATH_RE.search(text)
        if not m:
            break
        d = D_RE.search(m.group(0))
        if not d or not is_full_canvas(d.group(1), w, h):
            break
        # Keep at least one path: stop if this is the only path left.
        if not PATH_RE.search(text, m.end()):
            break
        text = text[:m.start()] + text[m.end():]
        removed += 1
    return text, removed


def main() -> int:
    args = [a for a in sys.argv[1:] if a != "--apply"]
    apply = "--apply" in sys.argv
    svg_dir = Path(args[0]) if args else Path("./out/svg")

    files = sorted(svg_dir.glob("*.svg"))
    if not files:
        print(f"No .svg files found in {svg_dir}", file=sys.stderr)
        return 1

    total_removed = changed = 0
    for f in files:
        text = f.read_text()
        new, removed = strip_background(text)
        if removed:
            changed += 1
            total_removed += removed
            note = f"removed {removed} background layer{'s' if removed > 1 else ''}"
            print(f"{'OK  ' if apply else 'DRY '} {f.name}: {note}")
            if apply:
                f.write_text(new)

    verb = "stripped" if apply else "would strip"
    print(
        f"\n{verb} {total_removed} background path(s) across {changed}/{len(files)} files"
        + ("" if apply else "  (dry run; pass --apply to write)")
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
