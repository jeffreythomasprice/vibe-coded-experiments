# GURPS 4e reference data

Everything here is copy-pasteable into a `.gcs` file or lookup-able when allocating points. Everything is GURPS 4e Basic Set unless otherwise noted (`B###` = Basic Set page).

## Attribute costs per level

Primary attributes (base 10):
- **ST** (Strength) — 10 pts per level
- **DX** (Dexterity) — 20 pts per level
- **IQ** (Intelligence) — 20 pts per level
- **HT** (Health) — 10 pts per level

Secondary characteristics (defaults shown; adjust with point costs):
- **Will** — base = IQ, 5 pts per level
- **Per** (Perception) — base = IQ, 5 pts per level
- **FP** (Fatigue Points) — base = HT, 3 pts per level
- **HP** (Hit Points) — base = ST, 2 pts per level (cost adjusted ±10% per Size Modifier)
- **Basic Speed** — base = (DX+HT)/4, 20 pts per +1.00 (so 5 pts per +0.25)
- **Basic Move** — base = floor(Basic Speed), 5 pts per level
- **Fright Check** — base = Will, 2 pts per level
- **Vision / Hearing / Taste & Smell / Touch** — base = Per, 2 pts per level

Derived rolls:
- **Dodge** = Basic Speed + 3 (rounded down)
- **Parry** = skill/2 + 3
- **Block** = skill/2 + 3

## Point budgets & campaign power levels

| Power level | Points | Disadvantage limit (typical) |
|---|---|---|
| Feeble | 25 | -25 |
| Average | 50 | -35 |
| Competent | 100 | -40 |
| Exceptional | 150 | -50 |
| Heroic | 200 | -50 |
| Super-heroic | 300+ | -75 or more |

**Quirks** — up to 5, worth -1 each. Pure flavor — don't count toward the disadvantage limit.

## Damage progression (Basic Set, B16)

Thrust/swing damage by ST:

| ST | Thrust | Swing |
|---|---|---|
| 1 | 1d-6 | 1d-5 |
| 2 | 1d-6 | 1d-5 |
| 3 | 1d-5 | 1d-4 |
| 4 | 1d-5 | 1d-4 |
| 5 | 1d-4 | 1d-3 |
| 6 | 1d-4 | 1d-3 |
| 7 | 1d-3 | 1d-2 |
| 8 | 1d-3 | 1d-2 |
| 9 | 1d-2 | 1d-1 |
| 10 | 1d-2 | 1d |
| 11 | 1d-1 | 1d+1 |
| 12 | 1d-1 | 1d+2 |
| 13 | 1d | 2d-1 |
| 14 | 1d | 2d |
| 15 | 1d+1 | 2d+1 |
| 16 | 1d+1 | 2d+2 |
| 17 | 1d+2 | 3d-1 |
| 18 | 1d+2 | 3d |
| 19 | 2d-1 | 3d+1 |
| 20 | 2d-1 | 3d+2 |
| 25 | 2d+2 | 5d-1 |
| 30 | 3d | 5d+2 |

Higher ST extrapolates; consult B16 or just let `gcs -validate` + `calc.thrust` / `calc.swing` fill it in. **Basic Lift** = ST² / 5 in pounds.

## Tech Levels (B511)

| TL | Era | Examples |
|---|---|---|
| 0 | Stone Age | bone, stone, early cave dwellers |
| 1 | Bronze Age | bronze, writing, chariots |
| 2 | Iron Age | Roman empire, early sailing ships |
| 3 | Medieval | longbows, plate armor, gunpowder discovery |
| 4 | Renaissance | matchlocks, galleons, Age of Sail |
| 5 | Industrial Rev. | steam power, early firearms, telegraph |
| 6 | Mechanized | WWI–WWII, radio, automobiles, flight |
| 7 | Nuclear | Cold War, jet age, early computers |
| 8 | Digital | modern day (our TL), smartphones, UAVs |
| 9 | Microtech | near-future, cybernetics, genetic engineering |
| 10 | Robotics | space colonies, strong AI |
| 11 | Exotic | antigrav, superluminal travel |
| 12 | Transcendent | sci-fi singularity |

Default campaign TL picks the baseline equipment available. Items at higher TL than the campaign are rare/expensive or unavailable.

## Humanoid body type (copy into settings.body_type)

Rolls 3d. Locations, roll ranges, hit penalties from B552. DR shown as 0 (armor supplies DR via equipment features).

```json
{
  "name": "Humanoid",
  "roll": "3d",
  "locations": [
    {"id": "eye", "choice_name": "Eyes", "table_name": "Eyes", "hit_penalty": -9, "description": "An attack that misses by 1 hits the torso. Only impaling, piercing, and tight-beam burning can target eyes — and only from front or sides. Injury over HP/10 blinds the eye.", "calc": {"roll_range": "-", "dr": {"all": 0}}},
    {"id": "skull", "choice_name": "Skull", "table_name": "Skull", "slots": 2, "hit_penalty": -7, "dr_bonus": 2, "description": "Miss-by-1 hits torso. Wounding x4. Knockdown rolls at -10. Critical hits use Critical Head Blow Table (B556).", "calc": {"roll_range": "3-4", "dr": {"all": 2}}},
    {"id": "face", "choice_name": "Face", "table_name": "Face", "slots": 1, "hit_penalty": -5, "description": "Miss-by-1 hits torso. Knockdown at -5. Critical hits use Critical Head Blow Table (B556). Corrosion gets x1.5 wounding.", "calc": {"roll_range": "5", "dr": {"all": 0}}},
    {"id": "leg", "choice_name": "Leg", "table_name": "Right Leg", "slots": 2, "hit_penalty": -2, "description": "Reduce pi+/pi++/imp wound multiplier to x1. Any major wound cripples the limb.", "calc": {"roll_range": "6-7", "dr": {"all": 0}}},
    {"id": "arm", "choice_name": "Arm", "table_name": "Right Arm", "slots": 1, "hit_penalty": -2, "description": "Reduce pi+/pi++/imp wound multiplier to x1. Any major wound cripples the limb. -4 if holding a shield.", "calc": {"roll_range": "8", "dr": {"all": 0}}},
    {"id": "torso", "choice_name": "Torso", "table_name": "Torso", "slots": 2, "calc": {"roll_range": "9-10", "dr": {"all": 0}}},
    {"id": "groin", "choice_name": "Groin", "table_name": "Groin", "slots": 1, "hit_penalty": -3, "description": "Miss-by-1 hits torso. Males suffer double shock from cr, -5 to knockdown rolls.", "calc": {"roll_range": "11", "dr": {"all": 0}}},
    {"id": "arm", "choice_name": "Arm", "table_name": "Left Arm", "slots": 1, "hit_penalty": -2, "description": "See Right Arm.", "calc": {"roll_range": "12", "dr": {"all": 0}}},
    {"id": "leg", "choice_name": "Leg", "table_name": "Left Leg", "slots": 2, "hit_penalty": -2, "description": "See Right Leg.", "calc": {"roll_range": "13-14", "dr": {"all": 0}}},
    {"id": "hand", "choice_name": "Hand", "table_name": "Hand", "slots": 1, "hit_penalty": -4, "description": "Reduce pi+/pi++/imp wound multiplier to x1. Major wound cripples.", "calc": {"roll_range": "15", "dr": {"all": 0}}},
    {"id": "foot", "choice_name": "Foot", "table_name": "Foot", "slots": 1, "hit_penalty": -4, "description": "Reduce pi+/pi++/imp wound multiplier to x1. Major wound cripples.", "calc": {"roll_range": "16", "dr": {"all": 0}}},
    {"id": "neck", "choice_name": "Neck", "table_name": "Neck", "slots": 2, "hit_penalty": -5, "description": "x2 wounding for crushing and cutting damage; cutting damage over half the victim's HP decapitates.", "calc": {"roll_range": "17-18", "dr": {"all": 0}}},
    {"id": "vitals", "choice_name": "Vitals", "table_name": "Vitals", "hit_penalty": -3, "description": "Imp and pi attacks get x3 wounding. Tight-beam burn gets x2. Other damage is treated as torso.", "calc": {"roll_range": "-", "dr": {"all": 0}}}
  ]
}
```

## Standard attributes template (copy into settings.attributes)

```json
[
  {"id": "st", "type": "integer", "name": "ST", "full_name": "Strength", "attribute_base": "10", "cost_per_point": 10, "cost_adj_percent_per_sm": 10},
  {"id": "dx", "type": "integer", "name": "DX", "full_name": "Dexterity", "attribute_base": "10", "cost_per_point": 20},
  {"id": "iq", "type": "integer", "name": "IQ", "full_name": "Intelligence", "attribute_base": "10", "cost_per_point": 20},
  {"id": "ht", "type": "integer", "name": "HT", "full_name": "Health", "attribute_base": "10", "cost_per_point": 10},
  {"id": "will", "type": "integer", "name": "Will", "attribute_base": "$iq", "cost_per_point": 5},
  {"id": "fright_check", "type": "integer", "name": "Fright Check", "attribute_base": "$will", "cost_per_point": 2},
  {"id": "per", "type": "integer", "name": "Per", "full_name": "Perception", "attribute_base": "$iq", "cost_per_point": 5},
  {"id": "vision", "type": "integer", "name": "Vision", "attribute_base": "$per", "cost_per_point": 2},
  {"id": "hearing", "type": "integer", "name": "Hearing", "attribute_base": "$per", "cost_per_point": 2},
  {"id": "taste_smell", "type": "integer", "name": "Taste & Smell", "attribute_base": "$per", "cost_per_point": 2},
  {"id": "touch", "type": "integer", "name": "Touch", "attribute_base": "$per", "cost_per_point": 2},
  {"id": "basic_speed", "type": "decimal", "name": "Basic Speed", "attribute_base": "($dx+$ht)/4", "cost_per_point": 20},
  {"id": "basic_move", "type": "integer", "name": "Basic Move", "attribute_base": "floor($basic_speed)", "cost_per_point": 5},
  {
    "id": "fp", "type": "pool", "name": "FP", "full_name": "Fatigue Points",
    "attribute_base": "$ht", "cost_per_point": 3,
    "thresholds": [
      {"state": "Unconscious", "expression": "-$fp", "ops": ["halve_move", "halve_dodge", "halve_st"]},
      {"state": "Collapse", "expression": "0", "explanation": "Roll vs. Will to do anything besides talk or rest; failure causes unconsciousness\nEach FP you lose below 0 also causes 1 HP of injury\nMove, Dodge and ST are halved (B426)", "ops": ["halve_move", "halve_dodge", "halve_st"]},
      {"state": "Tired", "expression": "round($fp/3)", "explanation": "Move, Dodge and ST are halved (B426)", "ops": ["halve_move", "halve_dodge", "halve_st"]},
      {"state": "Tiring", "expression": "$fp-1"},
      {"state": "Rested", "expression": "$fp"}
    ]
  },
  {
    "id": "hp", "type": "pool", "name": "HP", "full_name": "Hit Points",
    "attribute_base": "$st", "cost_per_point": 2, "cost_adj_percent_per_sm": 10,
    "thresholds": [
      {"state": "Dead", "expression": "round(-$hp*5)", "ops": ["halve_move", "halve_dodge"]},
      {"state": "Dying #4", "expression": "round(-$hp*4)", "explanation": "Roll vs. HT to avoid death\nRoll vs. HT-4 every second to avoid unconsciousness\nMove and Dodge halved (B419)", "ops": ["halve_move", "halve_dodge"]},
      {"state": "Dying #3", "expression": "round(-$hp*3)", "explanation": "Roll vs. HT to avoid death\nRoll vs. HT-3 every second to avoid unconsciousness\nMove and Dodge halved (B419)", "ops": ["halve_move", "halve_dodge"]},
      {"state": "Dying #2", "expression": "round(-$hp*2)", "explanation": "Roll vs. HT to avoid death\nRoll vs. HT-2 every second to avoid unconsciousness\nMove and Dodge halved (B419)", "ops": ["halve_move", "halve_dodge"]},
      {"state": "Dying #1", "expression": "-$hp", "explanation": "Roll vs. HT to avoid death\nRoll vs. HT-1 every second to avoid unconsciousness\nMove and Dodge halved (B419)", "ops": ["halve_move", "halve_dodge"]},
      {"state": "Collapse", "expression": "0", "explanation": "Roll vs. HT every second to avoid unconsciousness\nMove and Dodge halved (B419)", "ops": ["halve_move", "halve_dodge"]},
      {"state": "Reeling", "expression": "round($hp/3)", "explanation": "Move and Dodge halved (B419)", "ops": ["halve_move", "halve_dodge"]},
      {"state": "Wounded", "expression": "$hp-1"},
      {"state": "Healthy", "expression": "$hp"}
    ]
  }
]
```

## Common advantages (Basic Set)

Always confirm exact text / modifiers via `gcs -search <name> -search-type=traits`. This is a cheat sheet for common picks.

| Name | Cost | Page | Notes |
|---|---|---|---|
| Absolute Direction | 5 | B34 | Always know N. +3 Navigation. |
| Acute Vision / Hearing / Taste & Smell / Touch | 2/level | B35 | Adds to Per-based sense. |
| Ally | varies | B36 | Custom; see pointcost table B38. |
| Appearance (Attractive) | 4 | B21 | +1 reactions (B21). |
| Appearance (Handsome/Beautiful) | 12 | B21 | +4 to members of other gender. |
| Charisma | 5/level | B41 | +1 reactions, +1 to Influence skills per level. |
| Combat Reflexes | 15 | B43 | +1 Active Defenses, +2 Fast-Draw, never freeze. |
| Contact | varies | B44 | Custom. |
| Danger Sense | 15 | B47 | Free Per roll to sense danger. |
| Eidetic Memory | 5 | B51 | Remember anything seen/heard. |
| Fearlessness | 2/level | B55 | +1 Fright/Will vs. fear per level. |
| Fit | 5 | B55 | +1 HT for HT rolls and recovery; NOT HT level. |
| Very Fit | 15 | B55 | Fit, plus halve FP loss from exertion. |
| High Pain Threshold | 10 | B59 | No shock penalty from injury. |
| Intuition | 15 | B63 | GM may give hints on decisions. |
| Language Talent | 10 | B65 | Flat +1 to all language skills. |
| Lightning Calculator | 2 | B66 | Instant arithmetic. |
| Luck | 15 | B66 | Reroll 1/hour. |
| Extraordinary Luck | 30 | B66 | Reroll every 30 min. |
| Ridiculous Luck | 60 | B66 | Reroll every 10 min. |
| Magery (0/1/2/3+) | 5 + 10/level | B66 | Needed for magic; Magery 0 allows spellcasting. |
| Night Vision | 1/level | B71 | Ignore 1 point of darkness penalty per level. |
| Rapid Healing | 5 | B79 | +5 HT on healing rolls. |
| Very Rapid Healing | 15 | B79 | +5 HT and heal 2x normal rate. |
| Talent (varies) | 5–15/level | B89 | Bonus to a skill category. |
| Unfazeable | 15 | B95 | No reaction to surprise/horror. |
| Voice | 10 | B97 | +2 to social/communication skills. |
| Wealthy | 20 | B26 | Starts with 5× starting wealth. |

## Common disadvantages (Basic Set)

| Name | Cost | Page | CR? | Notes |
|---|---|---|---|---|
| Bad Sight | -25 | B123 | — | Corrective lenses mitigate. |
| Bad Temper | -10 | B124 | yes | Roll vs. CR to avoid outburst. |
| Callous | -5 | B125 | — | -1 reactions, no empathy. |
| Code of Honor (Gentleman's) | -10 | B127 | — | |
| Code of Honor (Professional) | -5 | B127 | — | |
| Code of Honor (Soldier's) | -10 | B127 | — | |
| Compulsive Behavior (varies) | -5 to -15 | B128 | yes | |
| Cowardice | -10 | B129 | yes | |
| Curious | -5 | B129 | yes | |
| Duty (dangerous/15 on 15-) | -15 | B133 | — | |
| Enemy | varies | B135 | — | |
| Greed | -15 | B137 | yes | |
| Gullibility | -10 | B137 | yes | |
| Honesty | -10 | B138 | yes | Must obey law. |
| Impulsiveness | -10 | B139 | yes | |
| Jealousy | -10 | B140 | — | |
| Lecherousness | -15 | B142 | yes | |
| Lazy | -10 | B143 | — | |
| Laziness | -10 | B143 | — | |
| Light Sleeper | -5 | B143 | — | |
| Loner | -5 | B142 | yes | |
| Miserliness | -10 | B144 | yes | |
| No Sense of Humor | -10 | B146 | — | |
| Obsession (varies) | -5 to -10 | B146 | yes | |
| Odious Personal Habit | -5 to -15 | B22 | — | -1 per -5 pts to reactions. |
| Overconfidence | -5 | B148 | yes | |
| On the Edge | -15 | B146 | yes | |
| Pacifism (Self-Defense Only) | -15 | B148 | — | |
| Pacifism (Cannot Harm Innocents) | -10 | B148 | — | |
| Sense of Duty (friends/nation/etc.) | -5 to -20 | B153 | — | |
| Shyness | -5/-10/-20 | B154 | — | -1/-2/-4 to social skills. |
| Stubbornness | -5 | B157 | — | |
| Truthfulness | -5 | B159 | yes | |
| Unluckiness | -10 | B160 | — | 1/session bad luck. |
| Vow (minor/major/great) | -1 to -15 | B160 | — | |

**Self-control rolls (CR)** — the number by which you must roll to resist the disadvantage. Cost multiplier:
- CR 6 → ×2 cost (almost always in control)
- CR 9 → ×1.5
- CR 12 → ×1 (standard)
- CR 15 → ×0.5 (usually out of control)
- CR 6/9/12/15 encoded as `"cr": 6` etc. in the trait JSON.

## Quirks (-1 each, max 5) — examples

Pick up to 5. They're just flavor + -1 pts each.
- Attentive (focuses hard on tasks)
- Broken-Minded (one specific scar)
- Chauvinistic
- Congenial
- Distractible
- Dreamer
- Habit (smokes / drinks coffee / etc.)
- Incompetence (very minor)
- Nosy
- Personality Change (when drunk/angry)
- Proud
- Responsive
- Sexy
- Staid
- Vow (minor)

## Skills — common picks by attribute/difficulty

| Skill | Difficulty | Notes | Page |
|---|---|---|---|
| Acting | IQ/A | | B174 |
| Administration | IQ/A | | B174 |
| Area Knowledge (region) | IQ/E | Defaults IQ-4. | B176 |
| Armoury/TL (specialty) | IQ/A | | B178 |
| Axe/Mace | DX/A | | B208 |
| Bow | DX/A | | B182 |
| Brawling | DX/E | | B182 |
| Broadsword | DX/A | | B208 |
| Camouflage | IQ/E | | B183 |
| Carousing | HT/E | | B183 |
| Climbing | DX/A | | B183 |
| Computer Operation/TL | IQ/E | | B184 |
| Crossbow | DX/E | | B186 |
| Diagnosis/TL | IQ/H | | B187 |
| Diplomacy | IQ/H | | B187 |
| Driving/TL (specialty) | DX/A | | B188 |
| Electronics Operation/TL | IQ/A | | B189 |
| Engineer/TL (specialty) | IQ/H | | B190 |
| Escape | DX/H | | B192 |
| Fast-Draw (specialty) | DX/E | | B194 |
| Fast-Talk | IQ/A | | B195 |
| First Aid/TL | IQ/E | | B195 |
| Guns/TL (Pistol/Rifle/Shotgun/etc.) | DX/E | | B198 |
| Hiking | HT/A | | B200 |
| Interrogation | IQ/A | | B202 |
| Intimidation | Will/A | | B202 |
| Jumping | DX/E | | B203 |
| Karate | DX/H | | B203 |
| Knife | DX/E | | B208 |
| Leadership | IQ/A | | B204 |
| Lockpicking/TL | IQ/A | | B206 |
| Mechanic/TL (specialty) | IQ/A | | B207 |
| Merchant | IQ/A | | B209 |
| Navigation/TL (specialty) | IQ/A | | B211 |
| Observation | Per/A | | B211 |
| Physician/TL | IQ/H | | B213 |
| Piloting/TL (specialty) | DX/A | | B214 |
| Pistol → Guns/TL (Pistol) | DX/E | | B198 |
| Riding (specialty) | DX/A | | B217 |
| Running | HT/A | | B218 |
| Savoir-Faire (specialty) | IQ/E | | B218 |
| Scrounging | Per/E | | B218 |
| Seamanship/TL | IQ/E | | B185 |
| Sex Appeal | HT/A | | B219 |
| Shadowing | IQ/A | | B219 |
| Shortsword | DX/A | | B209 |
| Sleight of Hand | DX/H | | B221 |
| Soldier/TL | IQ/A | | B221 |
| Stealth | DX/A | | B222 |
| Streetwise | IQ/A | | B223 |
| Survival (environment) | Per/A | | B223 |
| Swimming | HT/E | | B224 |
| Tactics | IQ/H | | B224 |
| Thrown Weapon (specialty) | DX/E | | B226 |
| Tracking | Per/A | | B226 |
| Two-Handed Sword | DX/A | | B209 |
| Wrestling | DX/A | | B228 |

**Point-to-level shortcut** — skill level at attr `A` with difficulty penalty `D` (E=0, A=-1, H=-2, VH=-3):
- 1 pt → A + D
- 2 pt → A + D + 1
- 4 pt → A + D + 2
- 8 pt → A + D + 3
- 12 pt → A + D + 4
- then +4 pt each for one more level

## Common equipment baselines (TL8 modern unless noted)

| Item | TL | Cost ($) | Weight | Notes |
|---|---|---|---|---|
| Clothing, ordinary | any | 120 | 2 lb | |
| Leather Jacket | 2 | 50 | 4 lb | DR 1 torso/arms (armor feature). |
| Light Body Armor (Kevlar) | 8 | 900 | 8 lb | DR 12 vs. pi, DR 6 vs. cut — verify via `-search`. |
| Ballistic Vest | 9 | 400 | 2 lb | DR 12 torso/vitals. |
| Combat Boots | 7 | 80 | 3 lb | DR 2 feet. |
| Flashlight (LED) | 8 | 20 | 0.5 lb | |
| First Aid Kit | 7 | 50 | 2 lb | +1 to First Aid rolls. |
| Multi-tool | 7 | 40 | 0.5 lb | |
| Smartphone | 8 | 500 | 0.5 lb | |
| Knife (large) | 0 | 40 | 1 lb | sw-2 cut / thr imp. |
| Broadsword | 2 | 500 | 3 lb | sw+1 cut / thr+1 imp. |
| Bow | 1 | 200 | 3 lb | thr imp; needs ST req. |
| Revolver, .38 | 5 | 200 | 1.9 lb | 1d+1 pi. |
| Pistol, 9mm | 7 | 500 | 1.7 lb | 2d+2 pi. |
| Rifle, 5.56mm (Assault Carbine) | 8 | 900 | 7.3 lb | 4d+2 pi, ROF 15, range 400/3,000. |
| Shotgun, 12G (Pump) | 6 | 240 | 8.1 lb | 1d+1 pi × 9 (shot). |

Always confirm exact values via `gcs -search <item> -search-type=equipment` — the library is authoritative. This table is a sanity check, not a catalog.

## Starting wealth (B27)

Default campaign starting wealth (TL-dependent; modified by Wealth advantage):
- **TL0–3**: $250
- **TL4**: $750
- **TL5**: $2,000
- **TL6**: $5,000
- **TL7**: $10,000
- **TL8**: $20,000
- **TL9**: $30,000
- **TL10+**: $50,000 and up

Wealth levels: Dead Broke (-25 pts, $0), Poor (-15, 1/5), Struggling (-10, 1/2), Average (0, 1×), Comfortable (10, 2×), Wealthy (20, 5×), Very Wealthy (30, 20×), Filthy Rich (50, 100×), Multimillionaire (+25/level beyond).

## Encumbrance (B17)

| Level | Weight up to | Move × | Dodge |
|---|---|---|---|
| None (0) | Basic Lift | 1.0 | full |
| Light (1) | 2× BL | 0.8 | -1 |
| Medium (2) | 3× BL | 0.6 | -2 |
| Heavy (3) | 6× BL | 0.4 | -3 |
| X-Heavy (4) | 10× BL | 0.2 | -4 |

## Reactions (B560)

Base reaction is 10 (Neutral). Modifiers from Appearance, Charisma, Voice, reputation, behavior, etc. Roll 3d:
- 3-6 Disastrous
- 7-9 Very Bad
- 10-12 Bad (reluctant to help)
- 13-15 Poor
- 16-18 Neutral
- 19-21 Good
- 22+ Very Good / Excellent
