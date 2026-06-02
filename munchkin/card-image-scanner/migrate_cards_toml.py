#!/usr/bin/env python3
"""One-shot migration of out/cards.toml to the new card schema.

- Drops OCR-pipeline bookkeeping fields.
- Renames top_extras -> below_title.
- Lifts monster level lines out of below_title into a new above_title string,
  normalized to "Level N" and with the OCR'd letter O fixed to digit 0.

Uses tomlkit so the hand-curated multi-line body strings keep their block style.
Run from anywhere: it resolves paths relative to this file.
"""
import re
from pathlib import Path

import tomlkit

TOML_PATH = Path(__file__).resolve().parent / "out" / "cards.toml"

DROP_KEYS = [
    "title_match_score",
    "bbox",
    "raw_line",
    "title_raw",
    "index_in_sheet",
    "row",
    "col",
]

# A below_title element that is *entirely* a level line, e.g. "LEVEL 12",
# "Level 4", or OCR'd forms like "LEVEL 2O" / "LEVEL IO" (letter O for zero,
# I/l for one). Anchored so sentences like "Will not pursue anyone of Level 5
# or" do NOT match.
LEVEL_RE = re.compile(r"^\s*level\s+([0-9oOiIlL]+)\s*$", re.IGNORECASE)
# OCR digit confusions in the level number.
_DIGIT_FIX = str.maketrans({"O": "0", "o": "0", "I": "1", "i": "1", "l": "1", "L": "1"})

# Output order; absent keys are skipped.
FIELD_ORDER = [
    "id",
    "source_image",
    "card_type",
    "title",
    "above_title",
    "below_title",
    "body",
    "bottom_left",
    "bottom_right",
    "card_image_path",
    "illustration_path",
]


def normalize_level(raw: str) -> str:
    """'LEVEL 2O' -> 'Level 20', 'LEVEL 12' -> 'Level 12'."""
    digits = LEVEL_RE.match(raw).group(1)
    digits = digits.translate(_DIGIT_FIX)
    return f"Level {int(digits)}"


def migrate_card(card):
    # 1. Drop bookkeeping fields (covers bbox/raw_line sub-tables too).
    for key in DROP_KEYS:
        card.pop(key, None)

    # 2. Rename top_extras -> below_title (preserve item representations).
    below = None
    if "top_extras" in card:
        below = list(card["top_extras"])
        card.pop("top_extras", None)

    # 3. Extract the level line into above_title.
    above = None
    if below is not None:
        for i, item in enumerate(below):
            if isinstance(item, str) and LEVEL_RE.match(item):
                above = normalize_level(item)
                below.pop(i)
                break

    # 4. Rebuild the table in the desired field order.
    new_card = tomlkit.table()
    for key in FIELD_ORDER:
        if key == "above_title":
            if above is not None:
                new_card["above_title"] = above
        elif key == "below_title":
            if below:  # drop if empty
                arr = tomlkit.array()
                arr.extend(below)
                arr.multiline(len(below) > 1)
                new_card["below_title"] = arr
        elif key in card:
            new_card[key] = card[key]
    return new_card


def main():
    doc = tomlkit.parse(TOML_PATH.read_text())
    cards = doc["card"]
    new_cards = tomlkit.aot()
    for card in cards:
        new_cards.append(migrate_card(card))
    doc["card"] = new_cards
    TOML_PATH.write_text(tomlkit.dumps(doc))
    print(f"Migrated {len(new_cards)} cards -> {TOML_PATH}")


if __name__ == "__main__":
    main()
