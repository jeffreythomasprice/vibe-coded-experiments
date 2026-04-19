# GCS JSON format reference

`.gcs` files are JSON. Current on-disk version is `5`. Older sheets can be `2` or `4`; `gcs -convert` upgrades them.

When writing a new sheet, always target version 5 and include the minimal skeleton at the bottom of this file.

## Root keys (v5)

```
version         5
id              string (stable unique id per character)
total_points    int  (the budget, e.g. 100, 150, 200)
points_record   [] of {when, points, reason}  (audit log of budget changes)
profile         {}  (biographical)
settings        {}  (attribute templates, body_type, page, block_layout, etc.)
attributes      [] of {attr_id, adj, calc:{...}}  (actual attribute values)
traits          [] of trait | trait_container  (advantages, disadvantages, quirks)
skills          [] of skill | skill_container | technique
equipment       [] of equipment | equipment_container  (carried/equipped gear)
spells          [] of spell | spell_container  (optional, omit if none)
notes           [] of note | note_container    (optional)
created_date    ISO 8601 with offset, e.g. "2026-04-19T12:00:00-04:00"
modified_date   same
calc            {}  (derived values: swing, thrust, basic_lift, move[], dodge[])
```

Note: equipment gets split by GCS's UI into "Equipment" (carried/equipped) and "Other Equipment" (stored elsewhere). The sheet also supports an `other_equipment` array; omit if empty.

## profile

```json
{
  "name": "Terra",
  "player_name": "laserpuppies",
  "tech_level": "3",
  "age": "35",
  "birthday": "November 10",
  "eyes": "Brown",
  "hair": "Black",
  "skin": "Tan",
  "handedness": "Right",
  "gender": "Female",
  "height": "5'8\"",
  "weight": "126 lb"
}
```

Only `name` really matters for validation; everything else is optional.

## settings

The settings block defines the rules the sheet uses. Most important subkeys:

- **`attributes`** — template defining what attributes exist and how they cost. The standard set (ST, DX, IQ, HT, Will, Fright Check, Per, Vision, Hearing, Taste & Smell, Touch, Basic Speed, Basic Move, FP, HP) is copy-pasted from `references/gurps-reference.md`.
- **`body_type`** — hit-location table. Standard "Humanoid" table in `references/gurps-reference.md`.
- **`damage_progression`** — `"basic_set"` for normal GURPS damage.
- **`page`** — PDF page settings.
- **`block_layout`** — order of sections in the sheet/PDF.
- **`default_length_units`** — `"ft_in"` (or `"m"`).
- **`default_weight_units`** — `"lb"` (or `"kg"`).
- Display flags: `user_description_display`, `modifiers_display`, `notes_display`, `skill_level_adj_display`, `show_spell_adj`, `exclude_unspent_points_from_total`.

See the minimal skeleton below for a standard settings block.

## attributes (actual values)

One entry per attribute template. `adj` is the delta from the template's `attribute_base`.

```json
[
  {"attr_id": "st", "adj": 1, "calc": {"value": 11, "points": 10}},
  {"attr_id": "dx", "adj": 2, "calc": {"value": 12, "points": 40}},
  {"attr_id": "iq", "adj": 0, "calc": {"value": 10, "points": 0}},
  {"attr_id": "ht", "adj": 0, "calc": {"value": 10, "points": 0}},
  {"attr_id": "will", "adj": 0, "calc": {"value": 10, "points": 0}},
  {"attr_id": "fright_check", "adj": 0, "calc": {"value": 10, "points": 0}},
  {"attr_id": "per", "adj": 0, "calc": {"value": 10, "points": 0}},
  {"attr_id": "vision", "adj": 0, "calc": {"value": 10, "points": 0}},
  {"attr_id": "hearing", "adj": 0, "calc": {"value": 10, "points": 0}},
  {"attr_id": "taste_smell", "adj": 0, "calc": {"value": 10, "points": 0}},
  {"attr_id": "touch", "adj": 0, "calc": {"value": 10, "points": 0}},
  {"attr_id": "basic_speed", "adj": 0, "calc": {"value": 5.5, "points": 0}},
  {"attr_id": "basic_move", "adj": 0, "calc": {"value": 5, "points": 0}},
  {"attr_id": "fp", "adj": 0, "calc": {"value": 10, "current": 10, "points": 0}},
  {"attr_id": "hp", "adj": 0, "calc": {"value": 11, "current": 11, "points": 0}}
]
```

`calc.value` and `calc.points` are actually computed by the gcs binary at load time — you can set them to reasonable placeholders and they'll be recomputed; they're mostly there so the sheet can render without first running calc. `current` appears only on pool-type attributes (FP, HP) and tracks remaining points/HP during play.

## traits

A trait is an advantage, disadvantage, quirk, or special ability. Two shapes: leaf (`type: trait`) and container (`type: trait_container`).

### Simple (flat-cost) trait

```json
{
  "id": "t9lxfCn84bo3idpX9",
  "type": "trait",
  "name": "Unluckiness",
  "reference": "B160",
  "notes": "Once per play session, the GM makes something go wrong for you.",
  "tags": ["Disadvantage", "Mental"],
  "base_points": -10,
  "calc": {"points": -10}
}
```

### Leveled trait

```json
{
  "id": "tP3jxCK2RQ5wT6wBq",
  "type": "trait",
  "name": "Charisma",
  "reference": "B41",
  "tags": ["Advantage", "Mental"],
  "can_level": true,
  "points_per_level": 5,
  "levels": 2,
  "calc": {"points": 10}
}
```

### Disadvantage with self-control roll

`cr` is the CR number (6/9/12/15). Lower CR = more often uncontrolled = bigger point refund.

```json
{
  "id": "tqkJB5OOYyDCf2dsS",
  "type": "trait",
  "name": "On the Edge",
  "reference": "B146",
  "base_points": -15,
  "cr": 12,
  "calc": {"points": -15}
}
```

### Trait with modifiers (enhancements/limitations)

```json
{
  "id": "t...",
  "type": "trait",
  "name": "Damage Resistance",
  "reference": "B46",
  "can_level": true,
  "points_per_level": 5,
  "levels": 3,
  "modifiers": [
    {
      "id": "m...",
      "type": "modifier",
      "name": "Tough Skin",
      "cost": -40,
      "cost_type": "percentage"
    }
  ],
  "calc": {"points": 9}
}
```

`cost_type` values: `"percentage"` (±% on cost), `"points"` (flat add), `"multiplier"` (multiply final cost).

### Trait that provides bonuses (`features`)

Features are bonuses the trait confers on other parts of the sheet.

```json
{
  "id": "t...",
  "type": "trait",
  "name": "Voice",
  "reference": "B97",
  "base_points": 10,
  "features": [
    {
      "type": "skill_bonus",
      "selection_type": "skills_with_name",
      "name": {"compare": "is", "qualifier": "diplomacy"},
      "amount": 2
    }
  ],
  "calc": {"points": 10}
}
```

Feature `type` values:
- `attribute_bonus` — `{type, attribute: "st"|"dx"|..., amount}`
- `skill_bonus` — add to matching skill(s)
- `weapon_bonus` — add to weapon damage/accuracy/etc.
- `dr_bonus` — `{type, locations: ["torso", ...], amount}`
- `reaction_bonus` — `{type, situation, amount}`
- `conditional_modifier` — `{type, situation, amount}`
- `cost_reduction` — reduces attribute cost

### Trait container

Groups children under a heading, commonly `"Advantages"` / `"Disadvantages"`. Container cost is the sum of children.

```json
{
  "id": "t_adv",
  "type": "trait_container",
  "name": "Advantages",
  "children": [ /* trait objects */ ]
}
```

### Linking to a library entry

When a trait was added from the library via the GCS UI, it carries a `source`:

```json
"source": {
  "library": "richardwilkes/gcs_master_library",
  "path": "Basic Set/Basic Set Advantages.adq",
  "id": "t_09iLAf_wQsT1YgF"
}
```

You can omit `source` for hand-written entries; the sheet still validates. Including it (copied from `-search` results) makes `gcs -sync` keep the item in step with library updates.

## skills

```json
{
  "id": "s1WgKka1x5dZbxaUH",
  "type": "skill",
  "name": "Carousing",
  "reference": "B183",
  "tags": ["Criminal", "Social", "Street"],
  "difficulty": "ht/e",
  "points": 4,
  "defaults": [{"type": "ht", "modifier": -4}],
  "calc": {"level": 13, "rsl": "HT+2"}
}
```

Fields:
- `difficulty` — `"<attr>/<diff>"`. attr ∈ `st|dx|iq|ht|will|per`. diff ∈ `e` (easy), `a` (average), `h` (hard), `vh` (very hard).
- `points` — points invested. Minimum 1. More points raises the skill level (1 pt = attr−diff-penalty, 2 pt = +1, 4 pt = +2, then +1 per doubling).
- `specialization` — optional string (e.g. `"Small Craft"` for Combat Piloting).
- `tech_level` — optional TL restriction (`"8"` etc.).
- `defaults` — array of `{type, modifier}` for default rolls. `type` can be an attribute (`"dx"`) or another skill (`{"type":"skill", "name":"Brawling", "modifier":-2}`).
- `defaulted_from` — set when the sheet chose a particular default to improve from.
- `source` — library reference (same shape as trait `source`).

Technique (trained combat maneuver, gated behind a base skill):

```json
{
  "id": "s...",
  "type": "technique",
  "name": "Targeted Attack",
  "difficulty": "h",
  "points": 2,
  "default": {"type": "skill", "name": "Broadsword", "modifier": -2},
  "limit": 0,
  "calc": {"level": 12, "rsl": "Broadsword+0"}
}
```

Skill container (just for grouping):

```json
{"id": "s_c1", "type": "skill_container", "name": "Physical Skills", "children": [ ... ]}
```

## equipment

```json
{
  "id": "eoGFKc8mCAby5SWsh",
  "description": "Emergency Survival Pack",
  "reference": "B288",
  "tech_level": "0",
  "tags": ["Camping and Survival Gear"],
  "base_value": "5",
  "base_weight": "1 lb",
  "quantity": 1,
  "equipped": true,
  "calc": {
    "value": 5,
    "extended_value": 5,
    "weight": "1 lb",
    "extended_weight": "1 lb"
  }
}
```

Notes:
- `description` (not `name`) is the display field.
- `base_value`, `base_weight` are strings (GCS parses them).
- `equipped: false` means carried-but-not-worn (DR doesn't apply, etc.).
- `calc.extended_*` accounts for quantity.
- Equipment containers (`type: equipment_container`) group items; `equipped` on a container cascades.

### Weapons embedded in equipment

A weapon-bearing item has a `weapons` array. Each weapon has `damage`, plus ranged/melee fields.

Melee weapon:

```json
{
  "id": "w...",
  "damage": {"type": "cut", "st": "sw", "base": "+1"},
  "usage": "Swung",
  "strength": "10",
  "reach": "1",
  "parry": "0",
  "defaults": [{"type": "skill", "name": "Broadsword"}],
  "calc": {"level": 12, "damage": "2d+1 cut", "parry": "10"}
}
```

Ranged weapon (firearm):

```json
{
  "id": "w...",
  "damage": {"type": "pi", "base": "4d+2"},
  "strength": "9†",
  "accuracy": "4",
  "range": "400/3,000",
  "rate_of_fire": "15",
  "shots": "30+1(3)",
  "bulk": "-3",
  "recoil": "2",
  "calc": {"level": 6, "damage": "4d+2 pi"}
}
```

Damage `type` values: `cr` (crushing), `cut` (cutting), `imp` (impaling), `pi-` (small piercing), `pi` (piercing), `pi+` (large piercing), `pi++` (huge piercing), `burn` (burning), `tox` (toxic), `cor` (corrosion), `tb` (tight-beam burn), `fat` (fatigue).

`damage.st`: `"thr"` uses character thrust, `"sw"` uses character swing; omit for flat-dice weapons like firearms.

### Armor (DR provider)

```json
{
  "id": "e...",
  "description": "Ballistic Vest",
  "reference": "B284",
  "tech_level": "9",
  "base_value": "400",
  "base_weight": "2 lb",
  "quantity": 1,
  "equipped": true,
  "features": [
    {"type": "dr_bonus", "locations": ["torso", "vitals"], "amount": 12}
  ]
}
```

Equipment `features` use the same feature types as trait features.

## spells

```json
{
  "id": "sp...",
  "type": "spell",
  "name": "Light",
  "college": ["Light/Darkness"],
  "power_source": "Arcane",
  "spell_class": "Regular",
  "casting_cost": "1",
  "casting_time": "1 sec",
  "duration": "1 min",
  "difficulty": "iq/h",
  "points": 1,
  "calc": {"level": 13, "rsl": "IQ+0"}
}
```

Spell containers group by college, same as other containers. Omit the `spells` root array entirely if the character has no spells.

## notes

Free-form text blocks rendered as their own section:

```json
{"id": "n...", "type": "note", "text": "**Session 3 injuries:** -2 HP lingering."}
```

Supports Markdown.

## calc (root)

Derived fields. Values the editor recomputes; you can set placeholders and they'll be overwritten:

```json
{
  "swing": "1d-2",
  "thrust": "1d-3",
  "basic_lift": "13 lb",
  "dodge_bonus": 0,
  "parry_bonus": 0,
  "block_bonus": 0,
  "move": [5, 4, 3, 2, 1],
  "dodge": [9, 8, 7, 6, 5]
}
```

The `move` and `dodge` arrays are indexed by encumbrance level (0=None through 4=X-Heavy).

## Minimal valid sheet (100 pts, ST/DX/IQ/HT = 10)

Drop this into a `.gcs` file; it should pass `gcs -validate`. All the long `settings.attributes` and `settings.body_type` blocks live in `references/gurps-reference.md` — paste them into the placeholders below.

```json
{
  "version": 5,
  "id": "REPLACE_WITH_UUID",
  "total_points": 100,
  "points_record": [
    {"when": "2026-04-19T00:00:00-04:00", "points": 100, "reason": "Initial points"}
  ],
  "profile": {
    "name": "New Character",
    "player_name": "",
    "tech_level": "3"
  },
  "settings": {
    "page": {
      "paper_size": "letter",
      "orientation": "portrait",
      "top_margin": "0.25 in",
      "left_margin": "0.25 in",
      "bottom_margin": "0.25 in",
      "right_margin": "0.25 in"
    },
    "block_layout": [
      "reactions conditional_modifiers",
      "melee",
      "ranged",
      "traits skills",
      "spells",
      "equipment",
      "other_equipment",
      "notes"
    ],
    "attributes": [ /* paste STANDARD_ATTRIBUTES from gurps-reference.md */ ],
    "body_type": { /* paste HUMANOID_BODY_TYPE from gurps-reference.md */ },
    "damage_progression": "basic_set",
    "default_length_units": "ft_in",
    "default_weight_units": "lb"
  },
  "attributes": [
    {"attr_id": "st", "adj": 0, "calc": {"value": 10, "points": 0}},
    {"attr_id": "dx", "adj": 0, "calc": {"value": 10, "points": 0}},
    {"attr_id": "iq", "adj": 0, "calc": {"value": 10, "points": 0}},
    {"attr_id": "ht", "adj": 0, "calc": {"value": 10, "points": 0}},
    {"attr_id": "will", "adj": 0, "calc": {"value": 10, "points": 0}},
    {"attr_id": "fright_check", "adj": 0, "calc": {"value": 10, "points": 0}},
    {"attr_id": "per", "adj": 0, "calc": {"value": 10, "points": 0}},
    {"attr_id": "vision", "adj": 0, "calc": {"value": 10, "points": 0}},
    {"attr_id": "hearing", "adj": 0, "calc": {"value": 10, "points": 0}},
    {"attr_id": "taste_smell", "adj": 0, "calc": {"value": 10, "points": 0}},
    {"attr_id": "touch", "adj": 0, "calc": {"value": 10, "points": 0}},
    {"attr_id": "basic_speed", "adj": 0, "calc": {"value": 5, "points": 0}},
    {"attr_id": "basic_move", "adj": 0, "calc": {"value": 5, "points": 0}},
    {"attr_id": "fp", "adj": 0, "calc": {"value": 10, "current": 10, "points": 0}},
    {"attr_id": "hp", "adj": 0, "calc": {"value": 10, "current": 10, "points": 0}}
  ],
  "traits": [],
  "skills": [],
  "equipment": [],
  "created_date": "2026-04-19T00:00:00-04:00",
  "modified_date": "2026-04-19T00:00:00-04:00",
  "calc": {
    "swing": "1d-2",
    "thrust": "1d-3",
    "basic_lift": "20 lb",
    "move": [5, 4, 3, 2, 1],
    "dodge": [8, 7, 6, 5, 4]
  }
}
```
