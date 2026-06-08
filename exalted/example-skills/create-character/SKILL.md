---
name: create-character
description: Walk the user through building an Exalted 2E Solar character sheet for the `ecs` tool. Solicit choices step-by-step, write the TOML, and validate it with the `ecs` binary. Any non-mechanical backstory the user provides goes into the TOML's `[[notes]]` array alongside everything else; XP earned in play goes into `[[xp_awards]]` (timestamped entries that must sum to `xp_earned`) so the running total has a per-session audit trail. Handles both fresh-chargen characters and post-chargen characters with earned XP to spend.
---

# create-character

Interactively build a character TOML the `ecs` (Exalted Character Sheet) CLI can validate and render.

This skill assumes the `ecs` binary is installed and available on `PATH`. If `ecs --help` fails, stop and tell the user — don't try to build, install, or guess at an alternative invocation.

## The binary is the only source of truth

Every piece of rules data (charms, backgrounds, spells, the chargen summary, the core-rules summary) is exposed as a subcommand of `ecs`. Do **not** read or reference loose files under `rules/`, `assets/`, or `src/` — query the binary instead.

### Subcommand catalog (every command this skill ever needs)

All commands accept `--output-format {text|json}` at the top level. `text` is human-readable markdown; `json` is the underlying struct(s). Examples below show the most useful form of each.

```bash
# ── Rules summaries ───────────────────────────────────────────────────────
# Chargen walkthrough (Steps 1–5, BP table, etc.)
ecs rules-markdown chargen

# Core-rules summary (combat, social, sorcery, anima, etc.)
ecs rules-markdown rules

# ── Backgrounds ───────────────────────────────────────────────────────────
# List every background as markdown (sorted by id; canonical ids appear in `(parens)` after the name)
ecs backgrounds

# Show one by id (markdown)
ecs backgrounds artifact

# All backgrounds as JSON (structs with id/name/kind/source/description) — useful for filtering with jq
ecs --output-format json backgrounds

# ── Charms ────────────────────────────────────────────────────────────────
# List every Solar/Abyssal/etc. charm (markdown). Long. Pipe to less or grep.
ecs charms

# One charm by id (markdown) — note: Excellencies are per-Ability, e.g. `first-archery-excellency`,
# not a generic `first-ability-excellency`.
ecs charms first-archery-excellency

# Filter by ability or keyword using JSON + jq
ecs --output-format json charms \
  | jq '.[] | select(.exalt_type=="solar" and .ability=="awareness") | {id,name,mins_ability,mins_essence}'

# ── Spells ────────────────────────────────────────────────────────────────
# All spells (markdown)
ecs spells

# One spell
ecs spells abjuration-of-the-maidens

# Filter to e.g. Terrestrial-Circle spells only
ecs --output-format json spells \
  | jq '.[] | select(.circle=="terrestrial") | .id'

# ── Validate ──────────────────────────────────────────────────────────────
# Human-readable
ecs validate /path/to/char.toml

# Machine-readable: {"ok": bool, "errors": [...], "notes": [...]} — use this in iteration loops
ecs --output-format json validate /path/to/char.toml

# ── Render ────────────────────────────────────────────────────────────────
# Markdown to stdout
ecs render /path/to/char.toml

# Markdown to a file
ecs render /path/to/char.toml -o /path/to/char.md

# Filled PDF (PDF requires `-o` because binary output isn't TTY-safe)
ecs render --format pdf /path/to/char.toml -o /path/to/char.pdf
```

Unknown ids exit with status 2 and print `no such {kind}: <id>` on stderr — handy for catching typos in scripts.

### When the summaries aren't enough

`rules-markdown chargen` and `rules-markdown rules` are *summaries*. For anything ambiguous, disputed, or not fully covered — odd interactions between Charms, edge-case BP/XP costs, Background nuances, Spell variants, errata — fall back to the full rule books via the `document-search` skill. **Always pass `--tags exalted`** so the search is scoped to the rule books and not unrelated documents. Prefer the rule books over the summaries when they disagree.

## 0. Before you start

Once per session, unless already cached in conversation, run:

```bash
ecs rules-markdown chargen
```

That gives you Steps 1–5, the bonus-point table, and Virtue Flaw lists. Enough for the common cases.

## 1. Frame the build up front

Ask the user, with `AskUserQuestion`, two things before doing anything else (you can do this in one question):

1. **Build type.** New chargen character (15 BP, no XP earned yet) **vs** existing character with some XP earned and spent. If XP, also ask how much has been earned and how much they expect to bank. For post-chargen characters with multiple sessions of history, ask whether they want to log each award (date + amount + what it was for) as `[[xp_awards]]` entries; if they do, those entries must sum to `xp_earned`.
2. **Output path.** Where to save the TOML — this is the user's choice and is typically *not* inside the exalted source repo. Suggest a path under the current working directory (e.g. `./characters/<name>.toml`) unless the user specifies otherwise. Everything about the character — mechanics and backstory alike — lives in this one file.

If they say "make something up" / "surprise me", pick reasonable defaults and tell them what you picked — don't grind them through every option.

## 2. Solicit choices in rule-book order

Walk Steps 1–5 of the chargen summary. For each, get just enough from the user to fill the TOML, then move on. Use `AskUserQuestion` whenever the choice set is small and bounded; use free chat for open-ended things (concept, motivation, anima totem, names).

Step-by-step prompts:

1. **Concept & Caste.** Caste is one of `Dawn / Zenith / Twilight / Night / Eclipse`. Concept + Motivation are short sentences. Anima totem is free text.
2. **Attributes.** Get the Physical/Social/Mental priority spread (8/6/4), then the within-category distribution. Reminder: every attribute starts at 1; the 8/6/4 are *additional*. None above 5 at creation.
3. **Abilities.** Caste Abilities are fixed by caste — list them back so the user knows. Get 5 Favored Abilities (no overlap with Caste). Then distribute 28 dots: ≥10 in Caste/Favored, ≥1 in each Favored, none above 3 without BP. Specialties optional.
4. **Backgrounds.** 7 dots total, none above 3 without BP, Cult ≤ 2 for Solars at creation. Each entry needs a canonical `id`; query `ecs backgrounds` to see the catalog or `ecs backgrounds <id>` to verify one. Some Backgrounds (Artifact, Allies, Familiar, Manse) can be taken multiple times — use a `label` field to distinguish instances.
5. **Charms.** 10 picks, ≥5 from Caste/Favored. Each Charm reference uses `kind = "lookup"` and a canonical `id`. Prereqs (Ability min, Essence min, prerequisite Charms in tree) must be met by final traits — `validate` will catch misses. If the user names a Charm by description rather than slug, use `ecs --output-format json charms | jq '.[] | select(.name|test("<keyword>";"i")) | {id,name,ability}'` to find the id, or `document-search --tags exalted` for the full text and adjacent context.
6. **Virtues & Flaw.** 5 additional dots across Compassion/Conviction/Temperance/Valor (each starts at 1). Primary Virtue must be 3+ and determines which Virtue Flaw list to pick from (Virtue Flaw lists are in the chargen summary).
7. **Finishing touches.** Intimacies (start with Compassion count; raise up to Willpower + Compassion at 3 BP each), Languages (one native family + dialect specialty free; Linguistics gives more), Willpower (= top two Virtues, raise with BP if desired), Essence (Solars start at 2).
8. **Identity & appearance** (Spark of Life). Hair, eyes, skin, distinguishing features, homeland, sex, anima totem. The homeland choice should be consistent with the native language family.

## 3. Build the TOML

There's no "copy this file" template — write the TOML directly. The schema is described below; `validate` will catch any drift.

### Top-level keys

```toml
caste = "Dawn"                          # Dawn|Zenith|Twilight|Night|Eclipse
favored_abilities = ["Awareness", "Dodge", "Stealth", "Survival", "Athletics"]
primary_virtue = "Compassion"           # Compassion|Conviction|Temperance|Valor
virtue_flaw = "CompassionateMartyrdom"  # PascalCase id from the primary virtue's flaw list
spells = []                             # array of charm-style refs (see below); usually [] at chargen
xp_earned = 0                           # total XP earned across the campaign
xp_banked = 0                           # = xp_earned - sum of all Xp-source purchases
hearthstones = []
# Optional: `[[xp_awards]]` entries below log each grant individually.
# When present, their `amount` values must sum to `xp_earned`. See "XP awards".
```

### Identity / appearance / languages

```toml
[identity]
name = "..." ; concept = "..." ; motivation = "..." ; personality = "..."
player = "..." ; chronicle = "..."

[identity.anima]
totem = "..."

[identity.appearance]
hair = "..." ; eyes = "..." ; skin = "..." ; distinguishing_features = "..."
homeland = "..." ; sex = "..."

[[languages]]
family = "Riverspeak"            # native language family
dialect_specialty = "Nexus"      # free dialect specialty within that family
native = true
```

Add more `[[languages]]` blocks (with `native = false`) for additional families purchased via Linguistics.

### `RatedTrait` (Attributes, Abilities, Virtues, Willpower, Essence, Background trait)

Every trait that takes dots uses the same shape:

```toml
[attributes.Dexterity]
base_dots = 1            # rules-given freebies: Attribute=1, Virtue=1, Ability=0,
specialties = []         # Essence=2 for Solars, Willpower=sum-of-top-two-Virtues

[[attributes.Dexterity.purchases]]
[attributes.Dexterity.purchases.source]
kind = "ChargenPriority"            # one of: ChargenPriority | BonusPoints | Xp

# A second purchase paid for with bonus points:
[[attributes.Dexterity.purchases]]
[attributes.Dexterity.purchases.source]
kind = "BonusPoints"
spent = 4
```

`source.kind` discriminators:
- `ChargenPriority` — drawn from the priority pools (the 8/6/4 attribute spread, 28 ability dots, 5 virtue dots, 7 background dots, 10 starting charms).
- `BonusPoints` with `spent = <bp>` — the 15-BP pool. Per-dot cost varies by trait and Caste/Favored status; see the BP table in `rules-markdown chargen`.
- `Xp` with `spent = <xp>` — post-chargen experience purchases. `validate` enforces the XP-cost table.

Final dots = `base_dots + len(purchases)`.

#### All attributes / abilities / virtues must appear

The TOML must have a section for **every** attribute (9), **every** non-Craft ability (24), and **every** virtue (4). For ones that get no dots beyond base, use:

```toml
[abilities.Larceny]
base_dots = 0
purchases = []
specialties = []
```

Attribute names use PascalCase: `Strength Dexterity Stamina Charisma Manipulation Appearance Perception Intelligence Wits`.
Ability names use PascalCase, with `MartialArts` (one word, no space). Full list: `Archery MartialArts Melee Thrown War Integrity Performance Presence Resistance Survival Investigation Lore Medicine Occult Athletics Awareness Dodge Larceny Stealth Bureaucracy Linguistics Ride Sail Socialize`.
Virtue names: `Compassion Conviction Temperance Valor`.

**Craft is the exception — it does NOT go in the `abilities` map.** Craft is a *family* of separately-rated abilities (Craft: Water, Craft: Fire, Craft: Magitech, …) that share a single Caste/Favored slot, so it lives in a top-level `[[crafts]]` array instead (see below). Don't add an `[abilities.Craft]` section; if a character has no crafts, just omit the array entirely.

#### Craft

Each elemental/material craft is its own top-level skill — a `[[crafts]]` entry with a `focus` string and a nested `[crafts.rating]` that reuses the normal `base_dots` / `purchases` / `specialties` shape. The first entry is the "primary" craft rendered on the sheet's single Craft row; any others render as extra rows. To mark Craft as Caste/Favored, put `"Craft"` in `favored_abilities` (the one slot covers every craft).

```toml
[[crafts]]
focus = "Water"          # the craft's focus; rendered as "Craft (Water)"

[crafts.rating]
base_dots = 0
specialties = []         # ordinary specialties still allowed, e.g. a sub-technique

[[crafts.rating.purchases]]

[crafts.rating.purchases.source]
kind = "ChargenPriority"
```

A craft's dots count toward the 28-dot ability pool and the ≥10-in-Caste/Favored minimum exactly like any other ability. **Elements/materials are the `focus`, never a specialty** — older sheets that encoded the element as a specialty on a single `[abilities.Craft]` entry are the legacy form; the binary auto-migrates them but with a blank focus, so write the `[[crafts]]` form directly.

### Attribute priority

```toml
[attribute_priority]
primary = "Physical"     # Physical|Social|Mental
secondary = "Social"
tertiary = "Mental"
```

### Willpower & Essence

```toml
[willpower]
base_dots = 6            # sum of top two Virtues, computed AFTER all virtue purchases
purchases = []
specialties = []

[essence]
base_dots = 2            # Solars start at 2
purchases = []
specialties = []
```

### Charms

```toml
[[charms]]
kind = "lookup"
id = "first-awareness-excellency"   # canonical slug; verify with `ecs charms <id>`
non_solar = false                   # true only for non-Solar Charms

[charms.source]
kind = "ChargenPriority"            # or BonusPoints { spent = N } / Xp { spent = N }

# Optional: any number of timestamped notes. See "Notes" below for the shape.
[[charms.notes]]
body = "Reflexive — use to detect ambushes during downtime travel."
created_at = "2026-04-18T20:15:00Z"
updated_at = "2026-05-02T19:30:00Z"
```

### Backgrounds

```toml
[[backgrounds]]
kind_tag = "lookup"
id = "resources"                    # canonical slug; verify with `ecs backgrounds <id>`
label = ""                          # distinguishing name (required for repeatable backgrounds:
                                    # Artifact, Allies, Familiar, Manse)

[backgrounds.trait]                 # RatedTrait, base_dots = 0
base_dots = 0
specialties = []

[[backgrounds.trait.purchases]]
[backgrounds.trait.purchases.source]
kind = "ChargenPriority"

# Optional: notes use the same shape on every entity.
[[backgrounds.notes]]
body = "Old Sifu Wen, master of the Silken Reed style. Lives in Nexus."
created_at = "2026-03-12T09:00:00Z"
updated_at = "2026-03-12T09:00:00Z"
```

### Spells

```toml
[[spells]]
kind = "lookup"
id = "blood-lash"                   # canonical slug; verify with `ecs spells <id>`

[spells.source]
kind = "Xp"                         # or ChargenPriority (sorcery swap) / BonusPoints
spent = 10

[[spells.notes]]                    # optional, same shape
body = "Learnt from a Nexus binder during the Festival of Waters."
created_at = "2026-04-30T22:10:00Z"
updated_at = "2026-04-30T22:10:00Z"
```

### Combos

```toml
[[combos]]
name = "Watchful Step"
charm_ids = [
    "first-awareness-excellency",   # must match the `id` of a CharmRef the
    "first-dodge-excellency",       # character already owns
]

[combos.source]
kind = "Xp"                         # or BonusPoints (BP cost = # of member Charms)
spent = 2                           # XP cost = sum of member min Ability ratings

[[combos.notes]]                    # optional, same shape
body = "Default opener when ambushed in alleys — boost JB then dodge."
created_at = "2026-05-04T18:45:00Z"
updated_at = "2026-05-09T12:00:00Z"
```

### Intimacies

```toml
[[intimacies]]
description = "The downtrodden"
kind = "Cause"                      # Cause|Person|Place|Principle|...
rating = 1

[intimacies.source]
kind = "Base"                       # or BonusPoints { spent = 3 } for extras beyond Compassion count
```

### Equipment, pool state, notes

```toml
[equipment]
weapons = []
other = []
artifacts = []

[pool_state]
personal_motes_spent = 0
peripheral_motes_spent = 0
committed_motes = []
willpower_temporary = 0
willpower_permanent_spent = 0
limit = 0

[pool_state.health_damage]
bashing = 0 ; lethal = 0 ; aggravated = 0

[pool_state.virtue_channels_used]   # empty table

# Top-level notes use the same shape as notes on any individual Charm /
# Background / Spell / Combo: a free-form `body` plus RFC3339 timestamps.
# Omit the `[[notes]]` block entirely if the character has none.
[[notes]]
body = "Session 1: party rescued the river-orphans of Nexus's Firewander District."
created_at = "2026-04-12T23:30:00Z"
updated_at = "2026-04-12T23:30:00Z"
```

### XP awards

Optional per-award history for `xp_earned`. Same timestamp shape as notes, with
an extra `amount` field. When **any** `[[xp_awards]]` entry exists, the sum of
`amount` values across all entries must equal `xp_earned` or `validate` will
emit `XpAwardSumMismatch`. Omit the block entirely to skip tracking (legacy
mode — `xp_earned` then stands on its own).

```toml
[[xp_awards]]
amount = 4
body = "Session 1 (Firewander orphans): 4 base + 0 stunt"
created_at = "2026-04-12T23:30:00Z"
updated_at = "2026-04-12T23:30:00Z"

[[xp_awards]]
amount = 6
body = "Session 2 (dockside ambush): 4 base + 2 dramatic"
created_at = "2026-04-19T23:30:00Z"
updated_at = "2026-04-19T23:30:00Z"
```

Prefer one entry per session/award rather than rolling many sessions into a
single big entry — they render as individual rows in the markdown XP History
table and as separate lines on the PDF appendix's Experience section.

## 4. Validate

Run validation and iterate until clean:

```bash
ecs --output-format json validate /path/to/char.toml
```

JSON shape: `{ "ok": bool, "errors": [...], "notes": [...] }`. `notes` are informational; `errors` are fatal. Fix every error before declaring done.

Common error names you'll see:

- `AbilityChargenDotsWrong` / `VirtueChargenDotsWrong` / `BackgroundChargenDotsWrong` — pool totals off.
- `CharmCountWrong` / `CasteFavoredCharmsTooFew` — chargen Charm budget.
- `CharmAbilityBelowMin` / `CharmEssenceBelowMin` / `CharmPrereqMissing` — Charm tree / prereq violation.
- `BonusPointsWrong` — BP ledger doesn't sum to 15 (or whatever was spent).
- `XpOverspent` / `XpCostWrong` / `XpBankedWrong` — XP accounting.
- `XpAwardSumMismatch` — `[[xp_awards]]` is non-empty but its `amount` values don't sum to `xp_earned`. Either add/remove an entry or correct the totals.
- `WillpowerNotFromVirtues` — `willpower.base_dots` doesn't equal top-two-Virtues.
- `VirtueFlawMismatch` / `PrimaryVirtueTooLow` — flaw not from primary Virtue's list, or primary Virtue under 3.
- `OldRealmRequiresLore` / `GuildCantRequiresBacking` — language prerequisites.
- `SolarCircleAtChargen` — Solar Circle spells aren't allowed at character creation.

The error message text usually says exactly what's wrong (expected vs. actual). If a particular error or XP cost is ambiguous, query `document-search --tags exalted` for the rule book passage rather than guessing.

## 5. Capture non-mechanical material as `[[notes]]`

Everything the user tells you about the character that **isn't a game mechanic** — backstory, the motivations behind the motivation, personality vignettes, NPCs in their life, prelude / exaltation details, plot hooks, GM threads — goes in the TOML's top-level `[[notes]]` array. Notes can also attach to any individual Background / Charm / Spell / Combo (see §3); use those when the material is specifically about that one item (who the Mentor is, what the artifact does, a tactical reminder for a Charm) rather than the character as a whole.

Each entry is a `body` plus RFC3339 `created_at` / `updated_at` timestamps. For chargen, set both timestamps to the current moment.

```toml
[[notes]]
body = """
Backstory: born in the Firewander District of Nexus, exalted at 19 during
a Guild raid on her dojo. Carries a grudge against the Guild factor who
ordered the raid.
"""
created_at = "2026-05-24T18:00:00Z"
updated_at = "2026-05-24T18:00:00Z"

[[notes]]
body = "Open thread: the factor's name is unknown; party suspects a Realm tie."
created_at = "2026-05-24T18:00:00Z"
updated_at = "2026-05-24T18:00:00Z"
```

Prefer multiple shorter `[[notes]]` entries (one per topic) over one giant blob — they render more readably on the markdown sheet and in the appended PDF Notes page, and individual entries can be updated independently as the campaign progresses.

Topics worth a separate note when the user has supplied content for them:

- Backstory / origin
- Personality
- Relationships & social ties
- Prelude / Exaltation
- Open threads & GM hooks

Skip any topic the user didn't actually provide material for — no empty placeholders.

## 6. Hand off

Once `validate` is clean, tell the user:
- the TOML path,
- a one-line summary (caste, concept, BP/XP spent),
- the renderable commands they'll likely want:
  - `ecs render <toml>` — markdown to stdout
  - `ecs render --format pdf <toml> -o <out.pdf>` — filled PDF

## Tips & gotchas

- **Don't invent Charm / Background / Spell IDs.** Verify every `id` with `ecs charms <id>` / `ecs backgrounds <id>` / `ecs spells <id>` before writing it into the TOML. A non-zero exit status means the slug is wrong.
- **Caste Charms vs Favored Charms.** Both count toward the "≥5 Caste/Favored" requirement and both get the BP/XP discount.
- **Excellencies share an `id` per Ability.** E.g. `first-awareness-excellency`, `second-stealth-excellency`. There's no generic excellency entry — the `charms` subcommand expands the per-Ability variants at startup, so pass the expanded slug.
- **Languages.** The free native family/dialect doesn't cost a Linguistics dot — Linguistics 0 still gets it. Each Linguistics dot adds *one more family*. Old Realm needs Lore 1+; Guild Cant needs Backing(Guild) 2+.
- **Solar Circle spells are forbidden at chargen** — `validate` will reject them via `SolarCircleAtChargen`.
- **If the user changes their mind partway** (e.g. swaps caste after picking Charms), re-derive Caste Abilities and re-check the ≥5-from-Caste/Favored Charm constraint before re-validating.
- **Binary missing?** If `ecs --help` exits non-zero, the user needs to install `ecs` and put it on `PATH` — surface that and stop, rather than trying to work around it.
- **Rule books over summaries.** The chargen and core-rules summaries shipped with the binary are *summaries*. For anything ambiguous, use `document-search --tags exalted` against the actual books and prefer that answer.
