# Exalted 2nd Edition — Gameplay Reference

Companion to `comprehensive_character_creation.md`. Covers the moment-to-moment
mechanics the character sheet expects you to know during play: initiative,
combat resolution, damage effects, movement, healing, Essence management,
Charm use, and mass combat.

Primary source: `Exalted 2E.pdf`. Page references appear inline as `(p.N)`.
Where the working character sheet's shorthand contradicted the book, the
corrected reading is flagged inline.

---

## 1. Initiative & Ticks

### 1.1 Join Battle (p.142)

Whenever an action could precipitate combat, every aware character makes a
**reflexive (Wits + Awareness) roll**, difficulty 1. Charms, stunts, and
specialty dice apply normally.

- The highest successes anyone rolls is the **reaction count** for the scene.
- Each character's **First Action** falls on `tick = reaction count − own successes` (clamped 0–6). A botch forces First Action = tick 6.
- Characters on tick 0 act immediately; combat then advances tick-by-tick.
- A bystander joining a fight already in progress uses Join Battle as a
  miscellaneous action of **Speed `(reaction count − successes)` (0–6), DV -0** (p.145). Speed-0 means they act on the same tick.
- A **surprised** target (lost an unexpected-attack contest) has **Dodge DV 0 and Parry DV 0** against the surprise attack (p.142).

### 1.2 Tick Track (p.141–142)

Ex2 uses **ticks** (~1 second each) instead of rounds. Combat starts at tick 0.

- After taking an action, the character must wait `Speed` ticks before acting again. Speed-5 attack on tick 0 → next action on tick 5.
- "Speed" on a weapon or action **is** the tick gap until DV refresh.
- **Simultaneous actions** on the same tick resolve as if nothing else that tick has happened — two combatants can kill each other on the same tick (p.142).
- **Flurries** let one character take multiple actions on the same tick. The flurry's total Speed = the **highest** Speed in the cascade; each component imposes its own DV penalty (all stack until next refresh); standard multiple-action dice penalties apply (p.143). A weapon may attack at most `Rate` times per flurry.
- **Extra-action Charms** create magical flurries that typically waive the multi-action dice penalty (p.142).
- **Reflexive actions** (Move, reflexive Charms) cost Speed 0 and may happen on any tick — they do not refresh DV or count as the "next action" for penalty-reset purposes (p.142).
- **Social combat** runs on **long ticks** of ~1 minute each (p.171).

---

## 2. Combat Resolution

### 2.1 Order of Attack Events (p.146)

The 10-step canonical sequence:

1. **Declare attack** — name the action; pre-declare any non-reroll Charms; if it bypasses dodge or parry, say so now.
2. **Defender declares response** — accept, dodge, or parry; pre-declare non-reroll defensive Charms.
3. **Attack roll** — `(Dex + Archery/Martial Arts/Melee/Thrown)`, difficulty 1, standard modifier order (p.124).
4. **Attack reroll** — Essence Resurgent or similar (only if no other Excellency on the attack). Each die rerolled once; if both rolls miss, the attack misses.
5. **Subtract external penalties / apply special defenses** — apply external penalties ending with the defender's DV; roll stunt/Charm dice that add to DV; resolve non-reroll defensive Charms.
6. **Defense reroll** — defender's reroll Charms (same Excellency restriction).
7. **Calculate raw damage** — `weapon base damage (usually Str + fixed) + remaining successes from step 5`. Note damage type (B/L/A).
8. **Apply Hardness and soak, roll damage** — if Hardness ≥ raw damage, absorbed; else ignore Hardness and subtract soak. Post-soak floor = greater of weapon minimum damage (default 1) or attacker's permanent Essence, never exceeding original raw. Roll that many dice (no botches, no double-10s); successes = health levels.
9. **Counterattacks** — defender's counterattack Charms run their own steps 1–8. Counterattacks cannot themselves be counterattacked.
10. **Apply results** — damage, knockdown, knockback, stunning, etc. (including the counterattack's effects).

### 2.2 Defense Values — DV (p.147–149)

Exalted and divine beings **round up**; mortals/heroic mortals **round down**.
The defender uses the higher of Dodge DV and Parry DV unless choosing the
inferior.

| DV | Formula |
|---|---|
| **Dodge DV** | `⌊(Dex + Dodge + Essence) / 2⌋ + 2` (subtract Mobility Penalty; War caps Dodge in mass combat) |
| **Parry DV** | `⌊(Dex + Ability + weapon Defense) / 2⌋ + 2` — Ability is almost always Martial Arts or Melee |

**Weapon Defense column.** Adds inside the parry formula *before* halving. A
staff (+2 Def) raises PDV; a sledge (−3 Def) drags it; unwieldy weapons can
push PDV negative. You cannot parry with a hand holding a weapon, so PDV
always tracks the currently-wielded weapon. Natural PDV bonuses: Punch +2,
Kick −2, Clinch — (no parry).

**Base vs. applied DV.** Applied = base + bonuses (shield, cover, height,
stunt successes) − penalties (action DV penalty, wound penalty, onslaught
−1 per prior hit in the same flurry, Mobility Penalty against Dodge DV only,
terrain, off-hand, etc.). All bonuses/penalties cumulative. Negative DVs
treated as 0 against attacks but tracked for later DV-boosting effects.

**Inapplicable defenses** (p.148): drop to **0 before** modifiers. Unarmed
characters cannot use PDV against lethal/aggravated attacks, and cannot parry
ranged attacks without a stunt/Charm (p.149). Choosing not to defend sets
both DVs to 0.

**Automatic defense vs. extras** (p.149): if DV exceeds an extra's full
attack dice pool, the attack auto-misses without a roll.

### 2.3 Mental Defense Values — MDV (p.174)

Standard rounding: Exalted/divine round up, mortals/heroic mortals round down.

| MDV | Formula |
|---|---|
| **Dodge MDV** | `⌊(Willpower + Integrity + pertinent specialty + Essence) / 2⌋` |
| **Parry MDV** | `⌊((Charisma or Manipulation) + Ability + pertinent specialty) / 2⌋ + 2`, where Ability is Investigation, Performance, or Presence |

**Charisma vs. Manipulation.** The *attacker* declares one in Step 1: Charisma
for honest persuasion, Manipulation for guile. The defender's Parry MDV uses
**the better of Charisma or Manipulation** by default — so the sheet's
"Honesty MDV" and "Deception MDV" boxes are not separate traits, but the
**same Parry MDV formula** with the Attribute chosen to best fit the defense
being mounted (a stunt that justifies a specific Attribute lets the defender
lock it in). Investigation parries Charisma-laced charm and
Manipulation-driven guile; Performance counters rhetoric; Presence counters
forceful argument.

**MDV modifiers** (p.174–175): action DV penalty (resets on next action),
onslaught from social flurries, coordinated social attacks, relative
Appearance (±[attacker Appearance − defender Appearance], capped at ±3),
Intimacy (−1/+1), Virtue 3+ (−2/+2), Motivation (−3/+3 — only the highest
penalty and highest bonus apply). Physical modifiers (cover, terrain, reach)
do not apply.

### 2.4 Base Soak (p.150–151)

| Damage type | Natural soak | Armor adds |
|---|---|---|
| Bashing | **Stamina** | armor's B soak |
| Lethal | **⌊Stamina / 2⌋** (Exalted and similarly resilient beings only; mortals get **0**) | armor's L soak |
| Aggravated | **0** for everyone | armor's aggravated soak = its lethal soak |

The sheet's garbled "Stamina / Stamina 2" is this split: full Stamina
bashing, half Stamina lethal. Mortals rely entirely on armor against lethal
(p.151).

**Piercing** halves armored soak (round down) before applying it; does **not**
reduce natural soak (p.151). **Hardness** compares against raw damage
*before* soak — if raw ≤ Hardness, attack does nothing; otherwise Hardness
is ignored entirely (p.150–151).

### 2.5 Default Unarmed Attacks (p.370)

| Attack | Speed | Acc | Damage | Defense | Rate | Mins | Tags |
|---|---|---|---|---|---|---|---|
| **Punch** | 5 | **+1** | +0B | **+2** | 3 | Str 1 | N |
| **Kick** | 5 | +0 | +3B | −2 | 2 | Str 1, Dex 2 | N |
| **Clinch** | 6 | +0 | +0B | — | 1 | Str 1 | C, N, P |

> **Sheet correction.** The Voidstate sheet shows Punch as Acc +0 / PDV +1.
> The book has **Acc +1 / PDV bonus +2** (p.370). Kick and Clinch on the
> sheet match the book.

Damage values are *bonuses* to `(Strength + successes)`: a Str 3 punch's raw
damage = 3 + accuracy successes (bashing). All three are **N (Natural)** —
undisarmable, use Martial Arts, parry only bashing without stunt/magic.
Clinch is also **C (Clinch Enhancer)** and **P (Piercing)**.

### 2.6 Weapon Stat Columns (p.373)

| Column | Meaning |
|---|---|
| **Speed** | Ticks until wielder's next action; sets flurry Speed too. |
| **Accuracy** | Modifier added to `(Dex + Ability)` on the attack. Clinch may sub Str for Dex. |
| **Damage** | Added to `(Str + attack successes)` for raw damage. `B`/`L`/`A` = bashing/lethal/aggravated. A `+XL/Y` split = base/minimum (overwhelming) or jab/charge (lance). |
| **Defense (PDV)** | Modifier inside the Parry DV formula. Can be negative. |
| **Rate** | Max attacks this weapon can contribute to one flurry. |
| **Minimums** | Attribute/Ability minima; each missing dot is −1 Acc, −1 Def, +1 Speed (Speed capped at 6). |
| **Range** (ranged/thrown) | Yards with no penalty. −1 at 2×, −2 at 2–3×, impossible beyond. |
| **Cost** | Resources rating to buy. |
| **Tags** (p.374) | `2` two-handed · `B` bow · `C` clinch enhancer · `D` disarming (+2 disarm) · `F` flame (no Str to dmg) · `L` lance · `M` martial arts · `N` natural · `O` overwhelming (innate min damage instead of 1) · `P` piercing · `R` reach · `S` single shot · `T` also throwable |

---

## 3. Actions

### 3.1 Common Actions — Speed / DV Penalty (p.142–145)

Reconciled from the **Action Options Summary** sidebar (p.142) and the
per-action text on pp.142–145.

| Action | Speed | DV Pen. | Notes |
|---|---|---|---|
| Join Battle (start of combat, reflexive) | — | -0 | Reflexive Wits + Awareness (p.142) |
| Join Battle (entering existing combat) | `reaction count − successes` (0–6) | -0 | Misc. action (p.145) |
| Ready Weapons (draw/sheathe) | 5 | -1 | Diceless; a draw-and-attack flurry uses weapon Speed (p.145) |
| Physical Attack | weapon's Speed | -1 | (p.143) |
| Social Attack | varies (Inv 5, Pres 4, Perf 6 long ticks) | **-2** | (p.171) |
| Coordinate Attack | 5 | -1 | (Cha + War), diff = ⌊participants/2⌋ (p.144) |
| Simple Charm | Charm's listed Speed (often 6) | per Charm | Sole non-reflexive action that tick (p.142) |
| Guard | 3 | **-0** | Optimised defense; can abort to anything (no DV refresh on abort) (p.143) |
| Move | **0** (reflexive) | -0 | Up to Dexterity yards/tick; cannot combine with Dash same tick (p.143) |
| Dash | 3 | **-2** | `Dex + 6 − Mob.Pen − Wound Pen` yards/tick; **cannot parry** without stunt/Charm (p.143) |
| Jump | 5 | -1 | One jump per flurry (p.145) |
| Rise from Prone | 5 | -1 | Normally automatic; prone = −1 external to non-reflexive physical (p.145) |
| Aim | 3 | -1 | +1 die per tick aimed; +3 dice at full cycle; cannot be flurried (p.142) |
| Miscellaneous | 5 | -1 / -2 / all DV | -1 default; full-focus on a task forfeits all positive DV (p.143) |
| Inactive | 5 | DV 0 (special) | Involuntary; helpless until conditions lift (p.143) |
| Flurry | highest Speed in the cascade | sum of components' DV penalties | Multi-action dice penalties apply; weapon limited to `Rate` attacks per flurry (p.143) |

### 3.2 DV Penalty Accumulation (p.143, 147)

- Every action imposes its DV penalty; **penalties stack on the same DV-cycle** (a 3-attack flurry stacks to −3 DV until refresh).
- **Applied DV** = `base + bonuses − accumulated action penalties − wound penalty − onslaught − etc.`
- **Penalty resets to 0** the moment the character's next action arrives (p.143, 147). Reflexive actions, Guard, and Aim do **not** refresh DV.
- **Onslaught** (p.147): each successful attack by the same attacker in the same flurry adds −1 to the defender's DV against that attacker, on top of action penalties; resets at the attacker's next action.
- **Wound penalty** (−0/−1/−2/−4) subtracts from both DVs while the wound persists (p.147, 150).
- **Coordinated attacks** (p.144): leader's coordination successes impose that many DV penalty (capped at attacker count).

---

## 4. Damage Effects

### 4.1 Knockdown (p.151)

Triggered when a single attack's **raw damage > Stamina + Resistance**.

- **Resisting roll:** reflexive `[Dex or Sta] + [Athletics or Resistance]`, difficulty 2.
- A deliberate **tackle** forces a knockdown check on **both** parties if the attack lands, and the target is **stunned even on a successful knockdown roll**.
- A **sweep** (chain, kick, staff) takes −2 Acc; if it hits at all (damage or no), the victim must check.
- **Knockback** (cinematic option, p.151): 1 yard / 3 dice of raw damage; lands prone. Solid object stops the slide; no extra damage from the throw itself.

**Prone consequences** (p.144, "Rising from Prone"):
- **−1 external penalty** on all non-reflexive physical actions while supine.
- Rising = miscellaneous action (-1 DV); diceless except under extreme conditions (earthquake, pitching deck).

### 4.2 Stunning (p.151–152)

Triggered when a single blow inflicts **HLs > defender's Stamina**.

- **Resisting roll:** reflexive `Stamina + Resistance`, difficulty = `(HL of damage − Stamina)`.
- **Failure:** −2 internal penalty (dice) on all non-reflexive rolls until the tick on which the attacker next acts — so duration scales with the attacker's Speed.
- A deliberate tackle (§4.1) bypasses this check entirely; victim is just stunned.

### 4.3 Wound Penalty (p.150; DV interaction p.147)

- The wound penalty equals the tier of the **lowest filled** (most-severe-tier) health level. It does **not** stack across tiers.
- Track: **−0, −1, −1, −2, −2, −4, Incap** (Ox-Body adds boxes within tiers).
- Greater damage **displaces** lesser damage to the bottom of the track, so the penalty reflects the worst hurt actually showing (p.150).
- **Internal penalty** — subtracts dice *before* the roll, on essentially every action (combat, social, mental, movement, etc.) (p.150).
- Wound penalty (and multi-action penalty) are the **two exceptions** to the "Essence 2+ may not be reduced below Essence dice" floor (p.124) — they always bite through.
- **DVs:** the *Defense Value Modifiers* table on p.147 subtracts wound penalty from **both** Dodge DV and Parry DV.
- **Movement:** subtracts yards from Move, Dash, and Jump (§5).
- **Infection rolls:** "wound penalties do subtract from Resistance rolls against infection, so severe wounds are more likely to become septic" (p.153).
- **Removal:** wound penalty disappears as HLs heal back (§6.1), or immediately if magic restores those HLs. Charms like Touch of Blissful Release (Medicine 2 / Essence 2, p.220) can suppress wound penalty temporarily.

---

## 5. Movement (p.127–128 jumping; p.144–145 move/dash)

All four values are **yards per tick** (~1 sec/tick). "Wound Pen" = current
health-level penalty (positive number subtracted); "Mob.Pen" = worn armor's
mobility penalty.

| Action | Formula (yd/tick) | Speed / DV | Notes |
|---|---|---|---|
| **Move** | `Dex − Wound Pen − Mob.Pen` (min 1) | 0 / -0 (reflexive) | No roll on normal ground; swimming/climbing halves rate and usually wants reflexive `Dex + Athletics`. |
| **Dash** | `Dex + 6 − Wound Pen − Mob.Pen` (min 2) | 3 / **-2**, **cannot parry** | Not reflexive. Cannot Move and Dash same tick (Dash supersedes). |
| **Jump (vertical)** | `Str + Athletics − Wound Pen − Mob.Pen` yards | Misc., -1 DV | Spending **1 WP adds +2 yards**; stunt/Virtue bonus dice add as yards. One jump per flurry. |
| **Jump (horizontal)** | **vertical × 2** yards | (same) | May Move *and* Jump same tick (no extra DV pen. for the Move). Short hops fit inside a Move. |

Long-distance / endurance running uses `Stamina + Resistance` instead (p.126).

---

## 6. Healing & Essence Recovery

### 6.1 Healing Rates (p.150–151)

Exalt rates below ("beings of similar resilience"); mortal rates are
roughly 4× longer (parenthetical where the book gives them).

| Damage | Tier | Exalt: rest | Exalt: light activity |
|---|---|---|---|
| **Bashing** | any | **1 HL / 3 hours** | (no doubled rate; mortals 1 HL / 12 hr) |
| **Lethal** | −0 | **6 hours** | 12 hours |
| **Lethal** | −1 | **2 days** | 4 days |
| **Lethal** | −2 | **4 days** | 8 days |
| **Lethal** | −4 / Incap | **1 week** | 2 weeks |
| **Aggravated** | (any) | heals at the lethal rate | same |

- "Double if not resting" is canon for lethal/aggravated only; book gives explicit `X rest / 2X normal activity` pairs at −0/−1/−2 and `1 wk / 2 wk` at −4/Incap.
- "Even an Exalt won't be able to do anything but rest if he's lying on death's door at Incapacitated." (p.151)
- **Aggravated** heals at lethal rate, but "only the strongest healing magic can mend such grievous injuries swiftly" — ordinary magical healing is *ineffective* against it. Armor aggravated soak = its lethal soak (p.151).
- Mortal lethal table: −0 = 1 day / 2 days; −1 = 1 wk / 2 wk; −2 = 2 wk / 4 wk; −4 & Incap = 1 month rest (cannot heal without rest).

### 6.2 Essence Regeneration (p.117; modifiers p.114–115; Underworld p.314–315)

**An Exalt recovers nothing during strenuous activity** (combat, manual
labor, hikes, forced marches). Otherwise:

| Condition | Rate |
|---|---|
| **At ease** (light show, leisurely stroll, courtly debate) | **4 motes/hour** |
| **Completely relaxed** (sleeping, receiving a massage) | **8 motes/hour** |

- **Personal first, then Peripheral.** No Peripheral motes return until Personal is full (p.117).
- **Attuned Manse (inside it):** `4 × Manse level` motes/hour (p.115).
- **Hearthstone (away from demesne):** `2 × Manse level` extra motes/hour while bearing the stone (p.115).
- **Cult bonus** (p.114, additive): Cult 2 → +2m/hr · Cult 3 → +3m/hr · Cult 4 → +4m/hr · Cult 5 → +6m/hr. (Cult 1 gives +1 temp WP/morning but no motes.)
- **Shadowlands:** living characters regain at **half rate** (p.314–315).
- **Underworld:** living characters **cannot regain Essence through rest or meditation at all** — must be fed or steal living Essence (p.314–315).
- Characters start every series with a **full** mote pool (p.117).

The book does not introduce a separate trance/meditation rule; "at ease" vs.
"completely relaxed" is defined entirely by the example pairs above.

---

## 7. Death & Dying (p.152)

- **Incap fills with bashing:** unconscious (Inactive) until the level heals. Further bashing converts the highest bashing wound into lethal, cascading.
- **Incap fills with lethal or aggravated:** character "hovers at the brink of death" and gains **Dying health levels = Stamina rating**.
- Additional bashing/lethal pushes into Dying as lethal; aggravated pushes in as aggravated.
- Each combat action the dying character is **forced Inactive** and suffers **1 unsoakable lethal** per interval.
- **Final death** = no health levels remain (all Dying filled and overflowed).
- **Stabilisation** (mortal medicine): `Wits + Medicine` at difficulty `5 + filled Dying levels`. Success heals **all** Dying levels and pins at Incap. Failure = immediate death. Requires Resources 4 surgical tools.
- Any **Charm or magic that restores HLs** auto-stabilises a dying character with no roll.
- **Death is permanent.** Even Solar Circle sorcery / god-Charms cannot resurrect; only reincarnation returns a soul.

**Bleeding** (p.152):
- Exalted: reflexive `Sta + Resistance`, diff 2, once per tick in combat (or every 5 sec out). Anyone with Medicine 1+ can stop it with an action.
- Mortals: `Wits + Medicine` at diff = HL of injury, one stanching per injury. Untreated bleeding = 1 unsoakable lethal every `Stamina` minutes.

---

## 8. Charm Mechanics

### 8.1 One Charm per Action (p.184)

> "Using a reflexive Charm counts as the one Charm the character can use in
> an action. ... if the character has already used a Charm in her action, she
> cannot use a reflexive Charm until her next action. ... if she uses one
> reflexive Charm in her action, she cannot use a different one until her DV
> refreshes. Combos (see p. 244) change these limits." (p.184)

Stated under Reflexive but applies generally: a Solar uses **one Charm per
action**. **Combos** are the canonical exception (p.244); **(Ability)
Essence Flow** (§8.7) is the other — Excellencies invoked through it no
longer count as the action's Charm.

### 8.2 Multi-use Supplemental / Reflexive (p.184)

- **Supplemental:** "The character can invoke the Charm multiple times in a flurry, with each invocation assisting a single action." (p.184)
- **Reflexive:** "Characters can use a reflexive Charm in any instant and as many times during her action as she wishes. She can even use it before her first action. She must pay the Charm's cost with each use." (p.184)

Both are still gated by §8.1 — multiple invocations of the **same** Charm
until next action / DV refresh.

### 8.3 Simple & Extra-Action Charms in mundane flurries (p.184)

- **Simple:** "The character cannot take multiple actions when using a simple Charm." (p.184)
- **Extra Action:** "The character can use the Charm only once per action and cannot add multiple actions by mundane means." (p.184)

Neither can appear in a normal multi-action mundane flurry.

### 8.4 The (Attribute + Ability) cap (p.185)

> "Charms can increase a Lawgiver's dice pools by only an amount equal to
> (the relevant Attribute + Ability). No combination of Charms can increase
> a Solar Exalt's dice pools by more than this amount. Charms that add
> automatic successes or remove penalties do not count as increases to a
> dice pool unless otherwise stated." (p.185)

For static values: "no combination of Charms, including the First
Excellency, can increase a static rating by more than half the
(Attribute + Ability)" (p.185). **Each purchased success counts as 2 dice**
for cap purposes (p.187).

- Specialties, weapon Accuracy, etc. are part of the *base* pool, not Charm bonuses — they don't count against the cap.
- "Charms that add automatic successes or remove penalties do not count against the cap *unless their text says otherwise*" (p.185).
- Assistance from another Exalt's Charms counts as Charm dice for cap purposes.
- Charms aiding non-Essence-wielding creatures cap at the creature's Ability (no specialties).

### 8.5 The Three Excellencies (p.183, 186–187)

All three: Mins (Ability) 1, Essence 1; Type Reflexive; Combo-OK; Duration
Instant; no prereqs. Each Ability has its own copy (e.g. *Melee Essence
Overwhelming*).

| # | Name | Cost | Step | Cap |
|---|---|---|---|---|
| 1st | Essence Overwhelming | 1m/die | Step 1 attacker / Step 2 defender | Dice added ≤ (Attr + Ability); on static values, each rolled *success* = +1 |
| 2nd | Essence Triumphant | 2m/success | Step 1 attacker / Step 2 defender | Successes added ≤ ⌊(Attr + Ability) / 2⌋ (each = 2 dice toward cap) |
| 3rd | Essence Resurgent | 4m flat | Step 4 attacker / Step 6 defender | Reroll the entire pool, take the better result; on static values, adds **⌊Ability / 2⌋** to the relevant DV |

3rd is **incompatible with 1st/2nd on the same roll**.

### 8.6 Infinite (Ability) Mastery (p.186–187)

- **Cost:** 2m+ (variable), 1 WP · **Mins:** (Ability) 4, Essence 3 · **Type:** Simple · **Duration:** One scene · **Prereq:** any (Ability) Excellency.
- "Each two motes committed to this Charm reduces the mote cost for the first three (Ability) Excellencies by one, to a minimum of 0." (p.186)
- The activation motes are **committed** for the scene (§9). So "per 2m committed" = pay up front, motes locked in your Committed total, and while committed every 2m shaves 1m off the cost of *each* invocation of that Ability's 1st/2nd/3rd Excellency.
- **Commitment cap:** Essence 3 may commit up to 6m; Essence 4+ unlimited (p.186).
- Discount applies to the **total** Excellency cost on a given roll (book example: 6m committed → 3m off the combined 1st+2nd cost on one roll).
- Activating Infinite Mastery itself is Simple — for that action you've used your Charm.
- **Incompatible with Essence Flow** — does not stack with the "Excellencies as innate powers" benefit (p.187).

### 8.7 (Ability) Essence Flow (p.187)

- **Cost:** — · **Mins:** (Ability) 5, Essence 4 · **Type:** Permanent · **Duration:** Instant · **Prereq:** any (Ability) Excellency.

> "Purchasing this Charm allows the Solar to invoke the First, Second and
> Third Excellencies for the relevant Ability as innate powers rather than
> Charms. This means that the character can use them even with a Combo that
> does not contain them or when she has already used a Charm for an action.
> However, she cannot use them out of place on the order of combat actions
> (see p. 145), nor may she apply the same Charm repeatedly to a single
> roll." (p.187)

- The (Attr + Ability) caps still apply (§8.4).
- 3rd still incompatible with 1st/2nd on the same roll.
- Must own each Excellency to invoke it through Essence Flow at the discount, but Essence Flow is bought **once per Ability**, not once per Excellency.
- **Incompatible with any cost-reduction effect** — does not stack with Infinite Mastery (p.187).

### 8.8 Charm Keywords (p.184)

| Keyword | Meaning |
|---|---|
| Combo-Basic | Combo-able only with Reflexive Charms (incl. Excellencies); not Simple, Extra Action, or Supplemental |
| Combo-OK | May be placed in a Combo |
| Compulsion | Special damage is a Compulsion effect |
| Counterattack | Charm is/contains a counterattack; cumulative −1 DV per counterattack until next action; no counter-counters |
| Crippling | Special damage is a Crippling effect |
| Emotion | Special damage is an Emotion effect |
| Form-type | Martial Arts Form; only one Form-type Charm active at a time |
| Holy | Damage imbued with the Unconquered Sun's judgment; extra impact vs creatures of darkness (p.192) |
| Illusion | Special damage is an Illusion effect |
| Knockback | Knocks target back; resistable by appropriate Charms |
| Obvious | Observers can tell a Charm is in use and roughly what it does; permanent Obvious Charms are obvious only while in use |
| Poison | Special damage is a Poison effect |
| Servitude | Special damage is a Servitude effect |
| Shaping | Integrity / Lore Charms treat its effects as Shaping |
| Sickness | Special damage is a Sickness effect |
| Social | Acts on the social time scale |
| Stackable | Multiple invocations are cumulative |
| Touch | Must touch the target; non-consenting targets need a (Dex + Martial Arts) attack |
| Training | Trains other characters; non-extras pay XP for any traits gained (debt allowed) |
| War | Acts on the war time scale, via Join War actions |

> "Native" and "Sorcerous" are not part of the core 2E Solar Charm keyword
> list — those terms come from later supplements or homebrew.

---

## 9. Committed Essence

### 9.1 Definition (p.183)

> "Other Charms have indefinite duration. The Chosen sustain these Charms
> with the power of their spirit. The motes of Essence spent on the Charm
> remain spent. These motes are known as committed Essence. While the
> Charm's effect persists, the Exalt cannot regain the motes of committed
> Essence." (p.183)

The character's effective max Essence pool drops by the committed total.
Book example: a 42-mote Solar who commits 10m to Hypnotic Tongue Technique
operates with a 32-mote ceiling until release, when the 10m reattach and
regenerate normally (p.183).

### 9.2 When Motes Commit

- **Attuning an artifact** — "When a character carries or wields an artifact, she must usually commit one or more motes of Essence to the use of the item, just as if she was sustaining the magic of a Charm that cost the same number of motes to activate." (p.380)
- **Indefinite-duration Charms** — activation cost commits for the duration (p.183).
- **Long-duration Charms with explicit commitment** — e.g. Hypnotic Tongue Technique (p.183 example), Infinite (Ability) Mastery (§8.6), Power-Awarding Prana (p.219, 15m + 1 WP, Duration Indefinite).
- **Hearthstone-setting artifacts** — Hearthstone amulet 1m (p.382); Hearthstone bracers 4m total / 2m per bracer (p.383); Dragon tear tiara 2m (p.383).
- **Sorcery does NOT commit** unless the spell text says so: "motes spent to power sorcery are not committed unless otherwise stated, even if the spell's effects linger beyond an instant." (p.183)

### 9.3 Releasing

- **Voluntary:** "An Exalt can end the effect of any of her Charms at any time" — releases the committed motes back into the regen queue (p.183). Artifact attunement may also be released at will (p.380; attunement also dissipates without daily skin contact).
- **Forced / lost:** the Charm being externally cancelled / dispelled releases its commitment; loss of consciousness, death, or Essence-loss below the Charm's requirement can end a Charm. Core 2E gives no single canonical rule on involuntary release — GM ruling unless a specific entry says otherwise.

### 9.4 Sheet Tracking

- Pools split into **Personal** = `(Essence × 3) + Willpower` and **Peripheral** = `(Essence × 7) + Willpower + Σ Virtues` (p.78).
- The "Committed" boxes adjacent to each pool's current/max track reduce that pool's effective maximum while the commitment persists; once released the motes re-enter the regen queue (Personal regenerates first, p.117).

> **Peripheral-first commitment caveat.** The 2E core does **not** state as
> a rule that commitment comes preferentially out of Peripheral. The only
> canonical Personal/Peripheral priority text is for *regeneration* (p.117).
> Peripheral-first commitment is widespread table convention (and is
> functionally smart — Peripheral spends flare the anima, and committing
> from there avoids forcing flare on every combat action) but is not
> citable from the 2E core. GM call.

### 9.5 Standard Attunement Costs (Ch. 8 Artifacts, p.385–393)

| Item | Attune | Source |
|---|---|---|
| Daiklave (Artifact 2) | 5 | p.385–386 |
| Short daiklave (matched pair, priced as one) | 3 each | p.386 |
| Grand daiklave (Artifact 3) | 8 | p.387 |
| Dire lance | 5 | p.387 |
| Goremaul | 5 | p.387 |
| Grand goremaul | 8 | p.387 |
| Grimcleaver | 5 | p.387 |
| God-kicking boots (matched pair) | 6 total | p.389 |
| Powerbow / artifact bow (standard) | 5 | p.391 |
| Hearthstone amulet (Artifact 1) | 1 | p.382 |
| Dragon tear tiara (Artifact 2) | 2 | p.383 |
| Hearthstone bracers (Artifact 2; both must be worn) | 4 (2 each) | p.383 |
| Mask (Artifact 2) | 5 | p.383 |
| Collar of Dawn's Cleansing Light | 1 | p.382 |
| Singing staff (Artifact 4) | 5 | p.395 |
| Lightning torment hatchets (pair, Artifact 3) | 6 total (3 each) | p.394 |
| Daiklave of Conquest (Artifact 5, Dawn-only) | 10 | p.395 |

> **Artifact armor caveat.** The Artifact 2/3/4/5 armor entries on p.393–394
> (Reinforced Buff Jacket, Reinforced Breastplate, Articulated Plate,
> Superheavy Plate) are OCR-corrupted in our PDF — the stat lines did not
> extract. Convention (reproduced in later books) is roughly **4 / 6 / 8 /
> 10 motes** for Artifact 2/3/4/5 armor. **Verify against a clean copy
> before play.**

**General attunement** (p.380): committing motes to an artifact takes ~20
minutes of handling; attunement dissipates without daily skin contact; once
attuned an artifact becomes "light in the hand" (an unattuned daiklave
weighs ~20 lb of deadweight). Items requiring committed Essence cannot be
activated by anyone with no motes to commit (p.382).

---

## 10. Mass Combat

Source: Ch. 4 (Drama and Systems), pp. 160–171. Verified against the
rulebook; where the working character sheet's shorthand was wrong or
ambiguous, the corrected reading is flagged.

### 10.1 Overview (p.160–161)

Mass combat abstracts large engagements into a clash of **units** rather
than individuals. Use it when (a) PCs are directing or fighting in a battle
of armies, or (b) the ST wants the outcome decided by rolls/strategy
rather than plot device. If the battle is just a dramatic backdrop for
personal-scale combat, do **not** use these rules.

- **Long ticks** replace standard ticks. 1 long tick ≈ 1 minute (p.160).
- Two kinds of unit (p.161):
  - **Solo unit** — a single character (Magnitude 0).
  - **Complementary unit** — a commander + all troops directly following her orders. Statistically the unit *is* its commander, with bonuses from troops/equipment.
- **The commander fights at the vanguard.** Anyone directing from the rear is a *general*, not a unit commander; generals can issue orders via relays but cannot personally rally or add prowess to a unit's roll (p.161).
- **Special characters** (PCs, named NPCs, sub-officers) embedded in a unit are tracked separately — protected from random casualty checks until the unit is destroyed; can act in sync or break out (p.163, 171).
- **War caps combat Abilities in mass combat:** Archery / Dodge / Martial Arts / Melee / Thrown are capped at War for all rolls. Heroes and sorcerer special characters may use their own War or the commander's, whichever is higher (p.164 sidebar).

### 10.2 Unit Traits (p.161–163)

Solo units use personal stats (Endurance = Stamina + Resistance). Complementary
units below:

| Trait | Formula / source | Notes |
|---|---|---|
| **Magnitude** | Look up from member count (table p.162; §10.3 below) | Size class |
| **Drill** | Look up from training quality (table p.162) | 0 Undrilled · 1 Barely · 2 Disciplined · 3 Crisp · 4 Crack · 5 Flawless |
| **Endurance** | `Drill + commander's Stamina` | Solo: Sta + Resistance. Subtract armor fatigue if troops haven't rested. At 0, −2 to all actions (p.162) |
| **Might** | Look up from member power (table p.163) + best equipment | 0 mortal · 1 god-blooded/thaumaturge · 2 young Terrestrial · 3 older Terrestrial / young Celestial · 4 experienced Celestial / 2CD · 5 elder Celestial. Equipment: +1 thaumaturgical, +2 basic magical, +3 essence-discharge / warstrider, +4 First Age artifact. Adds bonus successes to attacks and to commander's effective Essence for targeting/defense (p.163, 168) |
| **Close Combat Rating** | ⌊(avg Dex + avg Melee/MA + weapon Accuracy) / 2⌋ | The sheet's "Attack" row |
| **Ranged Attack Rating** | ⌊(avg Dex + avg Archery/Thrown + weapon Accuracy) / 2⌋ | Whichever fits the unit's weapons |
| **Close Combat Damage** | ⌈(avg Str + weapon Damage) / 3⌉ | Adds to raw damage of commander's close attacks |
| **Ranged Attack Damage** | ⌈(avg Str + weapon Damage) / 3⌉ | Same, ranged |
| **Armor** | ⌈(avg lethal soak) / 3⌉ | Mobility penalty = avg across members, not commander's |
| **Hardness** | avg across members | All must independently meet that rating (innate, magical, or equipment) |
| **Morale** | `min(avg member Valor, commander's Valor)` | Mounted units also capped by avg steed Valor if lower. Automata / walking dead have perfect Morale |
| **Health** | commander's max HL track | Starts every battle full, regardless of commander's current HLs (p.167). Tracked separately from commander's personal HLs |

The unit's Close/Ranged Rating **adds bonus successes equal to (Rating
capped by commander's War, plus Might) to the commander's roll** — it does
not replace it (p.168). **Minimum damage on a successful unit attack =
attacker's Magnitude** (not commander's Essence) (p.167).

**Special characters cap:** Magnitude × 2 max per unit (commander not
counted). Three roles:
- **Hero** — sub-officer; can replace fallen commander, attack as solo, break away with a sub-unit, lend Close Combat Rating (capped by commander's War).
- **Sorcerer** — lends Ranged Rating (capped by commander's War); can make independent ranged attacks; cannot break away or take command.
- **Relay** — drummers/signalmen. Units of Magnitude 3+ need **one relay per dot of Magnitude** or they're locked to Unordered formation at −2 effective Drill (p.163–164).

### 10.3 Magnitude Table (p.162)

| Magnitude | Members | Equivalent |
|---|---|---|
| 0 | 1 | Solo |
| 1 | 2–10 | Fang |
| 2 | 11–75 | Scale(s) |
| 3 | 76–150 | Talon |
| 4 | 151–300 | Wing |
| 5 | 301–650 | Dragon |
| 6 | 651–1,250 | — |
| 7 | 1,251–2,500 | — |
| 8 | 2,501–5,000 | Legion |
| 9 | 5,001–10,000 | First Age Legion |

Each step beyond 9 doubles the cap. Counts assume extras (3 HL each);
heroic mortals (7 HL) count as ~2 members. Heroes' Magnitude rounds up
to a multiplier of 3; Exalted / magical beings count for substantially
more (p.162).

> **Sheet-shorthand correction.** The cryptic "Close avg×40 / Relaxed
> avg×70 / Skirmish avg×100 / Unordered avg×30" on the working sheet is
> **NOT** a Magnitude formula. Those numbers are the **movement-rate
> multipliers** from the Move action table on p.166 (see §10.5). They have
> nothing to do with computing Magnitude.

### 10.4 Formations (p.164, 168)

Density, not geometric shape. Denser formations need higher Drill. Switching
takes the Change Formation action (§10.5).

| Formation | Drill req. | Description |
|---|---|---|
| **Unordered** | 0 | Loose mob behind a charismatic leader. Default for barbarians and any unit whose relays have been killed. |
| **Skirmish** | 1 | Spread out (~staff length apart). Mobile; resistant to ranged volleys; weak in close. |
| **Relaxed** | 1 | Default. Fingertip-spacing with arms wide. No bonuses or penalties. |
| **Close** | 2 | Shoulder-to-shoulder. Best synergy and morale in close combat; vulnerable to AoE. |

| Formation | DV from shield/cover | Other DV | Opponent's effective Magnitude vs. you | Hesitation diff. mod |
|---|---|---|---|---|
| **Close** | ×2 vs. close attacks; also **doubles unit's Close Combat Rating and the Close-Combat War cap** | — | Opponent **doubles** Magnitude for **ranged** bonuses | **−2** |
| **Relaxed** | ×2 vs. ranged only | — | — | — |
| **Skirmish** | ×2 vs. all attacks | **+3 DV vs. ranged** | Opponent **doubles** Magnitude for **close** bonuses (**triples** if attacker also in Close) | **+2** |
| **Unordered** | — | — | — | **+2** |

All four sheet shorthand items confirmed against the book.

### 10.5 Join War (p.164–165)

Replaces Join Battle for unit commanders. Initiating Join War prompts every
unit that sees the aggression to reflexively Join Battle in response (p.164).

- **Complementary unit:** `(Wits + War) − Magnitude`
- **Solo / independent special characters:** `(Wits + Awareness)`

So the sheet's "Wits + Awareness, or Wits + War − Magnitude" is correct —
which one depends on whether you're commanding a unit or acting solo.

**Surprise / ambush in mass combat:** requires actual concealed terrain — a
visible unit can never "surprise" another. Conceal roll = `(Dex + Stealth) −
Magnitude`, resisted by best `(Per + Awareness)` of any commander or special
character in any unit that might spot it (p.165).

### 10.6 Mass-Combat Action Options (p.165–166)

All Speed values in **long ticks**.

| Action | Speed | DV | Roll | Notes |
|---|---|---|---|---|
| **Move** | 0 | — | none | Yards per long tick = base movement rate × formation multiplier: **None/Solo ×100, Skirmish ×100, Relaxed ×70, Close ×40, Unordered ×30** (these are the sheet's misattributed numbers). Base rate comes from avg member Dex minus avg armor mobility — *not* commander's stats |
| **Dash** (charge / forced march) | (same as Move) | −2 | Cha + War, diff = `(Magnitude − Drill)` min 1 | Must be in a formation other than Unordered. Mounted units **double** Speed. Endurance drops immediately by avg armor fatigue. (Sheet's "Speed 3" looks like an error — book ties Dash to Move speed.) |
| **Guard** | 3 | — | none | Standard when advancing under ranged fire |
| **Inactive** | 5 | special | none | Only when forced (e.g. wide-area sleep spell). Rarely relevant for complementary units (can't be KO'd or grappled) |
| **Change Formation** | 5 | −1 | Cha + War (best relay may sub if higher), diff = `(Mag − Drill)` min 1 | +1 diff if attacked since last action; +2 instead if currently engaged. New formation's effects apply immediately. Unit may still take its normal move on the *next* tick. May be used reactively after taking missile fire to drop to Skirmish — resolve damage first, then roll; success forces a rout check at the new formation's modifier (p.165–166) |
| **Disengage** | 0 | — | `(Wits + War + Drill) − Magnitude`, diff = `(enemy Drill + 3)` | Reflexive. Required only to break off from a unit you're engaged with in close combat — but only if you actually want to retreat (you can take other actions while engaged) |
| **Turn** | 3 | −1 | Cha + War (or best relay), diff = `(Mag − Drill)` min 1 | Only needed for turns >90°. Sub-90° turns are reflexive. Attack from directly behind grants attacker unexpected-attack benefits |
| **Split Unit** | 3 | −1 | no roll for the split; both old & new unit then roll Cha + War, diff = `(own Mag − Drill)` min 1, +2 if engaged | Need a hero to command the new unit. Parent loses at least 1 Magnitude. New unit goes Unordered without enough relays. Failed rolls trigger hesitation |
| **Expel Special Character** | 0 | — | none | Reflexive. Special character becomes solo unit; cannot refuse, but may immediately turn around and challenge the commander to a duel |
| **Merge Units** | 3 | −1 | both commanders Cha + War, diff = `(own Mag − Drill)` min 1 | If either fails, merge fails and failing unit hesitates. Excess special characters either leave as solos or fade into ranks (unreachable for the rest of battle). Friendly *transfer* of special characters between adjacent units uses the same action at difficulty 1 |
| **Signal Units** | 3 | −0 | none | Sends one coded order to up to `(number of relays)` other units |
| **Rally** | 4 | −1 | Cha + (War or Performance), diff = `(Mag − Drill)` min 1 | Commander steps from the ranks (relay may sub). Pick one on success: **Organization** (promote rank-and-file to relay; capped Mag × 2 specials), **Numbers** (+1 Magnitude — must have lost Magnitude this battle or be drawing from a larger allied unit that did; ST veto if no real reinforcements), **Second Wind** (restore Drill points of Endurance, min 1; can't exceed Magnitude) |
| **Spell** | 5 | −2 | (as cast) | Any one spell, any Circle, in lieu of any other action. If commander casts, whole unit's action is consumed (they provide cover). Spells with >5 min casting are not accelerated |

**Flurry:** legal, but a unit may still only attack **one** target unit per
action — flurries just let you hit it repeatedly (p.167).

### 10.7 Special Characters in Units (p.163–164, 166–167, 171)

- **Embedding cap:** Magnitude × 2 specials per unit (commander not counted). Three roles per §10.2.
- **Personal Charms / damage flowing into the unit:** the commander may use reflexive and supplemental Charms on the unit's actions even though they're single-Charm activations — abstracted as the commander's example rippling through troops (Excellencies for dice, Adamant Skin Technique to save a HL on the unit's track, etc.). ST adjudicates per Charm but is encouraged to be lenient (p.164). Heroes lend Close Combat Rating (War-capped); sorcerers lend Ranged Rating (same cap).
- **Protection from harm:** unless **specifically targeted** with a called shot, specials and commander **automatically survive** until the unit loses its last dot of Magnitude — at which point all remaining specials die defending the commander, whose own HL track is whatever it was at battle start (less successful called shots) (p.163, 167). They are also exempt from the random casualty roll that rank-and-file survivors must make (p.169).
- **Targeting them:** a called shot at the commander or a named special character takes an external penalty equal to **½ the unit's Drill or Magnitude (round up)** — *higher* of the two if in any formation, *lower* if Unordered. Attack **must be ranged** unless the attacker's unit has been engaged with the target's unit on a previous long tick. Damage subtracts from the named character's *personal* HL track, not the unit's. Sorcerers double their unit's Magnitude when calculating that defensive penalty (p.166–167).
- **Acting in sync:** by default a special acts whenever the unit acts and gets all unit benefits. The player may instead Join War as if a solo unit (Wits + Awareness) and act on her own tick schedule until Guard or "fall back in line" resyncs her (p.171).
- **Acting independently:** the special **loses all unit benefits** except called-shot protection. Sorcerers always retain unit positioning for range purposes. Heroes who want to attack a unit other than the one their unit is engaged with must first be Expelled (becoming a solo unit) (p.171).
- **Leaving:** Expel Special Character (Speed 0, −0 DV, reflexive, no roll) ejects to solo. Cannot refuse but may immediately initiate a duel.
- **Duels:** if two engaged units' champions agree (or one forces the other via a successful called-shot that deals no damage), both step **outside** war time. Auto Join Battle, normal tick-by-tick personal combat until one is dead/incapacitated or both withdraw, then long ticks resume. Scene-length Charms persist. Killing a commander in a duel applies all the usual commander-death consequences. Meddling in a duel is dishonorable and triggers a +2-difficulty hesitation test on the meddler's unit (p.167).
- **Hero replacement:** if the commander dies, a hero **must** take command, or the unit begins to dissolve under the "Mob Rule" sidebar (p.169): goes Unordered, loses relays and effective specials, recalculates stats around an extra-quality leader (possibly only 3 HL per Magnitude dot), continues whatever it was doing, starts taking automatic rout checks at cumulatively increasing difficulty once that task finishes.

### 10.8 Hesitation Rolls (p.169)

A **hesitation roll** is the rout check a unit makes when it experiences a
triggering setback. It is **not** a per-tick check; it fires on specific
events.

- **Pool:** unit's **Morale** (dice).
- **Modifier:** **+ (Magnitude − Drill)** dice — negative for high-Drill, low-Mag units (disciplined elites are *harder* to break).
- **Difficulty:** standard 1, modified by both the **trigger** and current **formation**.

**Formation difficulty mods** (from §10.4): Close **−2**, Skirmish **+2**,
Unordered **+2**, Relaxed **+0**.

**Trigger difficulty mods** (p.169):

| Trigger | Diff. mod |
|---|---|
| Suffering Magnitude loss from damage | +0 |
| Receiving the first ranged attack since the unit's last action | +0 |
| Receiving a ranged attack from flame or Essence weapons | +1 |
| Being the subject of a spell | +1 per Circle |
| Becoming engaged with an enemy unit | +0 |
| …that enemy is superior or led by a supernatural being | +1 |
| …that enemy is overwhelmingly superior or composed of supernatural beings | +2 |
| Successfully disengaging from an enemy unit | +1 |

Other triggers scattered through the chapter: knockback/knockdown hitting
the whole unit (p.164), failed Split/Merge rolls (p.166–167), reactive
Change Formation under missile fire (p.166), special characters meddling in
a duel (+2, p.167), cumulative leaderless-unit checks under Mob Rule (p.169).

**Result:** on success, nothing. On failure the unit **hesitates** — cannot
move until next action, and **immediately loses one dot of Magnitude per
success it fell short by**. The Magnitude loss resets the unit's HL track
to full but may force extra relays back into the rank and file. This is the
more common way units are ground down in Exalted — not by raw damage but by
failed hesitation bleeding off Magnitude.
