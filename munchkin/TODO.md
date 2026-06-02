## 1. Missing card — needs to be scanned

- **`Convenient Addition Error`** (Treasure / "Go Up a Level"). It is **not in
  the scan at all** — there is no loot slot for it. Treasure count is 73/74.
  Locate the physical card, add it to the source sheets, then re-run the scanner.
  - Note: this was previously mis-reported as `Steal a Level`. Steal a Level is
    present — it's `loot4_01` (top-middle of `loot4.jpg`); the OCR only caught
    its "…A LEVEL" line, which matches both go-up-a-level cards, so it had been
    mislabeled. `loot4_01`'s title is now corrected to "Steal a Level" (its body
    text is already captured in that card's `top_extras`).

## 3. Full-card crops that produced empty body text — need re-OCR

Title set manually, but `body` is empty and there is no `title_raw`:

| Slot | Card |
|------|------|
| `loot2_06` | Flask of Glue |
| `loot3_07` | Invoke Obscure Rules |

(`loot9_00` had the same empty-body issue plus a crop problem — both now fixed,
see §2.)

## 4. Body text quality — decide whether to re-OCR or leave

Many `body` fields are usable but imperfect. These were **not** auto-edited
because the correct full text wasn't independently verified. Decide whether to
re-run OCR (preferred) or hand-correct. Systematic issues observed:

- **"Stuff" → "Stutt" / "Stutf"** on many monster cards
  (e.g. `door3_08`, `door5_03`, `door5_04`, `door6_05`, `door6_08`, `door5_01`).
- **"1" rendered as `|` or `I`** throughout ("Lose | Level", "LosE I LEVEL",
  "draw | extra Treasure").
- **Stray art/number fragments injected into body**, e.g. `Vy`, `2 /`, `4 /`,
  `\\`, `Je /`, `( ¢ AS /`, `PP ix / sa /`, `te / ONE /`, `% / = MMT) <> /`.
- **Dropped / mangled words**: `door1_02` Super Munchkin (dropped
  "disadvantages"), `door1_04` Cleric (dropped "gives"), `door8_05` Warrior
  (dropped "+1"), `door6_06` Harpies (dropped "Stuff:"), `loot7_00`
  Doppleganger ("pars" → "person"), `loot8_02` Magic Missile (dropped "either"),
  `loot8_03` ("'ting" → "during"), `door11_00` Mate ("at to Run Away").
- **Truncated bodies** where text overlaps the illustration band, e.g.
  Wizard cards (`door1_01`, `door1_03`, `door4_04` cut at "…other monsters in
  the combat,"), `door10_03`, `door10_04`, `door11_02`, `door3_07`, `door6_02`.





vectorize the extracted card images


make a card renderer


commit card-image-scanner/out, maybe under a better path