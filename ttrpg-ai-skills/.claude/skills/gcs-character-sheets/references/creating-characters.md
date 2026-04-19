# Creating a new GCS character

Use this workflow when the user asks to build a new sheet from scratch.

## 1. Clarify intent (batch into one message)

Before writing anything, confirm:

1. **Point total** — common tiers: 100 (competent), 150 (exceptional), 200 (heroic), 250+ (cinematic).
2. **Tech Level** — TL3 medieval, TL4 renaissance, TL6–7 WWII-to-modern, TL8 present-day, TL9 near-future, TL10+ sci-fi. See `gurps-reference.md` for the full table.
3. **Disadvantage limit** — usually -40 for 100 pts, -50 for 150+. Don't exceed.
4. **Concept** — one or two sentences so you know what traits/skills to reach for.
5. **Destination path** — where does the `.gcs` file go?

## 2. Start from the minimal skeleton

Copy the skeleton from `json-format.md` (bottom of file), paste in `STANDARD_ATTRIBUTES` and `HUMANOID_BODY_TYPE` from `gurps-reference.md`, set `profile.name`, `profile.tech_level`, and `total_points`.

Generate `id` values using Python `secrets.token_urlsafe(12)` or similar — any unique string works. The live GCS app uses base64-ish short IDs (e.g. `t9lxfCn84bo3idpX9`).

## 3. Allocate attributes

Pick levels for ST/DX/IQ/HT using the costs in `gurps-reference.md` (ST 10/lvl, DX 20/lvl, IQ 20/lvl, HT 10/lvl). Then adjust secondary characteristics only if the concept demands it (high Will for a stubborn officer, high Per for a scout).

Set `adj` = level − 10 (or for secondaries, level − default). Fill `calc.value` with the resulting level and `calc.points` with `adj × cost_per_point` (`gcs -validate` will recompute anyway, but this keeps the sheet coherent).

For FP and HP, also set `calc.current` equal to `calc.value`.

## 4. Search-first for every trait, skill, and piece of equipment

**Never hand-write trait/skill names from memory.** Always:

```
gcs -search "<name or fragment>" -search-type=traits
gcs -search "<name or fragment>" -search-type=skills
gcs -search "<name or fragment>" -search-type=equipment
```

From the result array:
1. Prefer the hit whose `source_file` contains `Basic Set/` (or whose `data.reference` starts with `B`) unless the user asked for a specific supplement.
2. Copy the `data` block into the sheet's `traits` / `skills` / `equipment` array, wrapping it with `"type": "trait"` (etc.) if the library entry doesn't already have it.
3. Keep the library `id`. It keeps `gcs -sync` working.
4. Optionally add a `source` block pointing back to the library file (see `json-format.md`).

If the user asks for a trait that doesn't exist in the library, build it by hand from `gurps-reference.md`'s cost tables — and set `reference` to the book page (e.g. `"B160"`) so the sheet still renders a citation.

## 5. Pick skills

Rule of thumb: 1 pt buys a skill at its default difficulty (attr - easy/avg/hard penalty). 2 pt = +1, 4 pt = +2, then one more level per doubling. See the shortcut table in `gurps-reference.md`.

For combat characters, front-load weapon skills at DX+1 or DX+2; for non-combat characters, spread skills broadly at DX or DX-1.

## 6. Equipment

Use `-search` with `-search-type=equipment`. For armor, confirm the `features` array includes a `dr_bonus` with the right locations. For weapons, confirm the `weapons` array is present with the correct `damage.type` and `damage.base` (or `damage.st: "thr"/"sw"` for ST-scaling melee).

Set `equipped: true` for worn/wielded items; otherwise `false`.

Total cost should stay within starting-wealth × Wealth-advantage-multiplier (see `gurps-reference.md`).

## 7. Validate and iterate

```bash
/home/jeff/workspaces/personal/gcs/gcs -validate <path>
/home/jeff/workspaces/personal/gcs/gcs -points <path>
```

Fix any error lines from `-validate`. `-points` will show whether `unspent` is 0 (balanced), positive (budget left to spend), or negative (over budget — user must cut).

Iterate until `unspent` is 0 and `spent` matches the intended `total_points`.

## 8. Optional: PDF export

```bash
xvfb-run /home/jeff/workspaces/personal/gcs/gcs -pdf <path>
```

Produces `<name>.pdf` in the current directory (or `-pdf-out <dir>` destination).

## Template: blank 100-pt human character

See `json-format.md` "Minimal valid sheet". Copy it, then apply this checklist:

- [ ] Set `profile.name`
- [ ] Set `profile.player_name`
- [ ] Set `profile.tech_level`
- [ ] Set `total_points`
- [ ] Replace `settings.attributes` placeholder with `STANDARD_ATTRIBUTES` from `gurps-reference.md`
- [ ] Replace `settings.body_type` placeholder with `HUMANOID_BODY_TYPE` from `gurps-reference.md`
- [ ] Adjust `attributes[]` entries (`adj` values)
- [ ] Add traits (search-first)
- [ ] Add skills (search-first)
- [ ] Add equipment (search-first)
- [ ] Run `-validate`; fix errors
- [ ] Run `-points`; confirm `unspent = 0`
- [ ] Optional `xvfb-run gcs -pdf` for hardcopy

## Tip: converting an old sheet

If the user hands you a pre-v5 sheet (`"version": 2` or `"version": 4`), run:

```
/home/jeff/workspaces/personal/gcs/gcs -convert <path>
```

This rewrites the file in place to v5. Make a backup first if the user cares about the original.
