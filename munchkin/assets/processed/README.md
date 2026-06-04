# Processed cards

This directory holds the cleaned-up, structured output of the card-image-scanner
pipeline. The raw scans live in `../raw`; here each individual card has been
cropped, OCR'd, hand-corrected, and described as structured data.

```
processed/
├── cards.toml          # the structured card data (source of truth)
├── images/
│   ├── raw/            # per-card cropped PNGs (full card + isolated illustration)
│   └── svg/            # vectorized illustrations (for rendering)
└── README.md           # this file
```

## `cards.toml`

A flat list of cards, one `[[card]]` table per card:

```toml
[[card]]
id = "door11_02"
source_image = "door11.jpg"
card_type = "door"
title = "King Tut"
above_title = ["Level 16", "Undead"]
below_title = [
    "Will not pursue anyone of Level 3 or below. Characters of higher levels lose 2 Levels, even if they escape.",
]
body = "Bad Stuff: Lose all your items and all cards in your hand."
bottom_left = "2 Levels"
bottom_right = "4 Treasures"
card_image_path = "images/raw/door11_02_card.png"
illustration_path = "images/raw/door11_02_illustration.png"
card_image_svg_path = "images/svg/door11_02_illustration.svg"
```

### Content fields (top-down layout)

These fields represent sections of the card face, listed here in the order they
appear from the **top of the card to the bottom**:

| Field                  | Position on card                            | Notes |
| ---------------------- | ------------------------------------------- | ----- |
| `above_title`          | Above the title                             | e.g. a monster's `Level`, an item's bonus / usage restriction |
| `title`                | The card's name / headline                  | Present on every card except the card backs |
| `below_title`          | Just under the title                        | Subtitle, combat modifiers, flavor or rules text |
| `card_image_svg_path` / `illustration_path` | The central artwork          | See [Image fields](#image-fields) — these are paths, not text |
| `body`                 | The main rules text block                   | |
| `bottom_left`, `bottom_right` | The two bottom corners, at the same y-level | Left and right corners respectively |

Every content field is **optional** — a card only carries the fields it actually
has.

### Plain string vs. array of strings

Any content field (`above_title`, `title`, `below_title`, `body`,
`bottom_left`, `bottom_right`) may be either:

- **A plain string** — rendered as a single run of text.
- **An array of strings** — rendered as separate lines, stacked vertically.
  `["foo", "bar"]` renders as:

  ```
  foo
  bar
  ```

So `above_title = ["Level 16", "Undead"]` puts `Level 16` on one line and
`Undead` below it, both above the title.

### Bottom corners by card type

`bottom_left` / `bottom_right` are generic corner labels; their meaning depends
on the card:

- **Monsters (`door` cards):** `bottom_left` is the Levels gained for defeating
  it, `bottom_right` is the Treasures awarded (e.g. `"2 Levels"`, `"4 Treasures"`).
- **Items (`loot` cards):** `bottom_left` can note hands occupied or size
  (e.g. `"1 Hand"`), `bottom_right` is the Gold Piece value
  (e.g. `"400 Gold Pieces"`).

### Image fields

| Field                 | What it is |
| --------------------- | ---------- |
| `card_image_path`     | The full cropped card image (PNG, in `images/raw/`). Present on every card, including backs. |
| `illustration_path`   | The card's illustration extracted in **raw** form (PNG, in `images/raw/`). |
| `card_image_svg_path` | Roughly the same illustration as `illustration_path`, but **vectorized** (SVG, in `images/svg/`). This is the form intended for rendering. |

`card_image_svg_path` and `illustration_path` are two representations of the same
artwork: the SVG is the render target, the raw PNG is the extracted source it was
traced from.

### Metadata fields

| Field          | What it is |
| -------------- | ---------- |
| `id`           | Unique card id, derived from the source scan and position (e.g. `door11_02`). |
| `source_image` | The original raw scan this card was cropped from (e.g. `door11.jpg`). |
| `card_type`    | `"door"` or `"loot"` — the two Munchkin deck types. |

### Card backs

Each deck has a single back card (`door-back_00`, `loot-back_00`). These carry
only `id`, `source_image`, `card_type`, and `card_image_path` — no title, body,
or illustration fields.
