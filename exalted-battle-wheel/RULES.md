# Exalted 2E Combat Rules — Reference for the Battle Wheel

Research notes on the Exalted 2nd Edition tick system, gathered from the core rulebook. This is
a precision reference for building a tick-tracking app, not a tutorial.

## Sources and citation convention

All rules below come from **Exalted 2E core rulebook** (`Exalted 2E.pdf` in the local corpus,
403 pages). Citations use **printed book page numbers**, matching the book's own cross-references.

> When re-searching with `document-search --tag exalted`, note the PDF page index is
> **printed page + 2** (verified: PDF page 155 = printed page 153).

Combat chapter: printed pp. 140–168 (Chapter Four, "Drama and Systems").
Weapon/armor tables: printed pp. 366–378 (Chapter Eight, "Panoply").

**Caveat on OCR:** the corpus copy is OCR'd (the PDF's embedded font encoding is broken, so
`pdftotext` returns garbage and the scan layer is all there is). The prose extracted cleanly, but
**the weapon stat tables did not**. Specifically:

- Several **Speed** digits are simply missing — marked `?` below.
- **Minus signs are routinely dropped**, so a Defense printed as `-2` scans as `2`. Where the
  book's prose independently confirms the sign (e.g. "a sledge (Defense -3)", p. 146) it is
  restored; elsewhere the sign is inferred from context and marked.
- A few cells are reconstructed from partial glyphs and are marked `?` even where a plausible
  value is given.

Verify the whole weapon table against a clean copy before hardcoding it. Everything outside §8.2–8.4
is direct prose and is reliable.

---

## 1. The tick model

### 1.1 Core concepts

- Combat time advances in **ticks**, each nominally **one second**. (p. 141)
- Combat **always advances from tick 0 forward, one tick at a time**, until battle ends. (p. 141)
- Every action has a **Speed** (how many ticks until the actor's next action) and a
  **DV penalty** (how much it degrades the actor's Defense Values until DV refreshes). (p. 141)
- Notation throughout the book: `Action (Speed/DV penalty)`, e.g. `Dash (3/-2)`.

### 1.2 The scheduling rule

> "Once a character takes her first action in combat, she must wait a number of ticks equal to
> the Speed rating of her action before she acts again. This delay resets with the Speed of each
> new action and forms the basic cycling structure of combat." (p. 141)

So: **`next_action_tick = current_tick + Speed`**.

The book's three suggested tracking methods all confirm this arithmetic:

- **Paper:** mark an X at the acting tick, then a second X "a number of tick rows down equal to
  the Speed rating of the action." (p. 140)
- **Dice:** set a die to the Speed on the acting tick; decrement at the *beginning* of every
  following tick; act when it reaches zero. (p. 140)
- **Counters:** pile of tokens equal to Speed on the acting tick; remove one at the beginning of
  every tick; act when the pile empties. (p. 140)

A **Speed 0** action therefore resolves on the current tick and does not consume the actor's
place in the cycle — this is what makes reflexive actions and Speed-0 Join Battle work.

### 1.3 Characters must act

> "Characters act for the first time in combat when the tick count reaches the point where they
> have their First Action, and they may take any action desired, **but they must act**. Doing
> nothing is itself an action, whether a character is waiting in a guard position or paralyzed."
> (p. 141)

There is **no "delay" or "hold" action** in 2E. The way to wait for a better moment is to take
**Guard** (Speed 3, DV -0) and abort out of it — see §4.

### 1.4 Simultaneity

> "When multiple characters act on the same tick, their actions occur simultaneously. Nothing
> actually happens until every action is rolled and the tick is concluded, so actions disregard
> the effects of 'previous' rolls made in the same tick. Therefore, two combatants can strike and
> even kill one another on the same tick." (p. 141)

Implication for the app: a tick is a **transaction**. All actions on tick N are declared and
rolled against the state as it stood at the *start* of tick N, and effects are applied at the
*end* of the tick. Two mutual kills on one tick is a legal, expected outcome.

The book offers three optional declaration orders for handling player metagaming (arbitrary
order; ascending `Wits + Awareness`; or free cooperative metagaming) — the Storyteller picks one
and tells players in advance. (p. 141) This is a UI/table-convention concern, not a rules
concern.

---

## 2. Starting a battle

### 2.1 Join Battle

1. A character declares a **Join Battle action** when she wants to do something that requires
   combat time. Doing so "projects hostility or at least intense physical readiness," which
   normally lets **everyone who can perceive her** also Join Battle. Uninvolved bystanders may be
   assumed not to. (p. 141)
2. If the initiator is hidden or concealing intent, resolve the **surprise check first**
   (see §10.2). Only those who beat the initiator get to Join Battle — unless someone who
   succeeds shouts a warning, in which case anyone who hears may Join Battle too. (p. 141)
3. **Join Battle is a reflexive roll of `Wits + Awareness`, standard difficulty** (difficulty 1).
   It benefits normally from Charms, stunts, and other bonuses. (p. 141)

### 2.2 Reaction count and First Action

> "The **reaction count** for the combat scene is a value equal to the highest number of successes
> rolled by anyone who simultaneously joins at the start of combat. […] The **First Action** of
> each character equals **(reaction count − successes)**, to a maximum value of 6. Any character
> who botches a Join Battle roll automatically has a First Action of 6." (p. 141)

Consequences:

- The reaction count is a **scene-level constant**, established once at the start of combat, and
  is reused later by anyone joining a battle in progress (§4, Join Battle miscellaneous action).
- The fastest character (or characters, on ties) has First Action **tick 0** and acts immediately.
- First Action is **clamped to [0, 6]**.
- If nobody notices a hidden assailant, that assailant still rolls Join Battle and **his successes
  establish the reaction count**. (p. 141)

### 2.3 Being attacked before your first action

> "Those attacked before their first action have their **normal DV**, but characters attacked
> **by surprise** have a Parry and Dodge DV of 0." (p. 141)

So "hasn't acted yet" is *not* a penalty. Only genuine surprise zeroes DV.

Characters begin combat **with their full DV**, subject to any standing modifiers. (p. 141)

---

## 3. DV refresh — the other half of the tick loop

This is the mechanic most easily gotten wrong, so it is worth stating precisely.

- Most actions impose a **DV penalty** that lasts until the actor's DV refreshes.
- > "This penalty disappears on the tick the character is next permitted to act." (p. 147)
- The social-combat chapter states the timing even more sharply: the penalty "vanishes when DV
  refreshes **immediately before** the character's next action." (p. 172)

So DV refresh happens at the **start** of the tick on which the character may act, *before* the
new action's penalty is applied. A character with a Speed 5 action taken on tick 3 is at reduced
DV for ticks 3, 4, 5, 6, 7 and refreshes at the top of tick 8.

**Things that do NOT refresh DV** (important — these are the exceptions the app must model):

| Case | Rule |
|---|---|
| Reflexive actions (incl. Move, reflexive Charms) | "Reflexive actions do not refresh a character's DV, nor do they count as true actions for the purposes of effects that last until a character's next action." (p. 142) |
| Aborting a **Guard** to another action | "This new action does not refresh DV but is a normal action in all other ways. Therefore, the character must wait for a number of ticks to pass according to the Speed of the new action to refresh DV and act again." (p. 143) |
| Aborting an **Aim** to attack the studied target | "the attack does not refresh DV, even though it counts as a normal action in all other respects." (p. 142) |
| Completing a **full Aim cycle** | "he still does not refresh DV" (p. 143) |

So Guard and Aim are both "DV-suspended" states: you keep the *current* DV until the follow-up
action's Speed elapses.

Two states that **do** get a clean refresh:

- **Inactive** ends abruptly when its cause withdraws: "On the next available tick, the character
  may act normally with **refreshed DV** and a full range of options." (p. 143)

---

## 4. Action catalog

The core summary box (p. 141), expanded with the detailed entries (pp. 142–145):

| Action | Speed | DV penalty | Reflexive? | Flurryable? |
|---|---|---|---|---|
| **Activate Charm / Combo / Power** | Varies (Simple defaults to 6) | Varies | Depends on Charm type | Depends on type |
| **Aim** | 3 | -1 | No | **No** |
| **Attack** | = weapon/maneuver Speed | -1 | No | Yes (up to weapon Rate) |
| **Dash** | 3 | -2 | No | — (and cannot parry at all) |
| **Flurry** | = highest Speed in the cascade | each action's own penalty, cumulative | No | (is the flurry) |
| **Guard** | 3 | **-0** | No | **No** |
| **Inactive** | 5 | Special (DV 0) | Involuntary | No |
| **Miscellaneous Action** | 5 | Varies (-0 to full DV forfeit) | No | Sometimes |
| **Move** | **0** | None | **Yes** | n/a (free) |

### 4.1 Aim (3/-1)

- Declare a specific target when the action is selected. (p. 142)
- **Abort to attack the studied target** on any tick after the first: **+1 die per tick spent
  aiming**. Aborting to anything else is impossible. The aborted attack does not refresh DV.
- If forced to do something else with the aborted action, lose **2 dice** (internal penalty from
  divided attention).
- **Completing a full aim cycle** (the whole Speed 3) lets the next action attack the studied
  target with **+3 bonus dice**.
- Instead of attacking, the character may **re-enter a new aiming cycle**. DV does not drop
  further, and no more bonus dice accrue, but the accumulated bonus stays **"banked" through as
  many aim cycles as desired** until used (attacking the target) or forfeited (any other action).
  This models "covering" a target. (p. 143)
- **Aim cannot be part of a flurry.**

App note: `Aim` is a stateful thing — it carries `(target, banked_dice)` across ticks.

### 4.2 Attack (Varies/-1)

The Speed of an attack **is the Speed of the weapon or attack maneuver used** (p. 143). See §8
for weapon data.

### 4.3 Dash (3/-2)

- Rate: **`Dexterity + 6 − wound penalties − armor mobility penalty`** yards/tick, **minimum 2**.
  (p. 143)
- Not reflexive. **Cannot parry at all** without a stunt or magic (on top of the -2 DV).
- Usually diceless; `Dexterity + Athletics` may be required for treacherous terrain.
- Alternate locomotion: swimming/climbing at **Dexterity** yards per tick (both usually need
  rolls); flying per the means of flight.

### 4.4 Flurry (Varies/Varies)

- **Speed** = the **highest Speed rating of any action** in the cascade. (p. 143)
- **DV penalty**: each action in the flurry imposes **its own** penalty, cumulatively.
- **Dice penalty**: normal multiple-action penalties (p. 124–125): with *N* actions, the first
  loses *N* dice, and each successive action loses one more.
  → 3 actions = **-3 / -4 / -5**.
- A weapon **cannot attack more times in a flurry than its Rate**. (p. 143)
- Some actions are barred from flurries (Aim, Guard).
- **Aborting a flurry:** if a chosen action becomes invalid (e.g., the target dies on the first
  blow), the character may abort. The flurry **ends** at that action — you cannot pick and choose
  among the remaining actions. The flurry's **Speed remains what it was when declared** (even if
  the longest action was one of those dropped), but the character takes DV penalties **only for
  the actions actually undertaken**. (p. 143)
- **Special-case Speed exception:** "a flurry that only involves a character drawing a weapon and
  using it for attacks uses **the Speed of the weapon**, even if the Speed is less than 5. This is
  an exception to the usual rules for determining flurry Speed." (p. 144)

### 4.5 Guard (3/-0)

- No DV reduction. The character dodges/blocks as best suits her training. (p. 143)
- **On any tick in which she is guarding**, she may **abort the defense and take any other action
  except Aim or another Guard**. The new action does not refresh DV but is otherwise normal —
  she must then wait its Speed to refresh DV and act again.
- **Guard cannot be part of a flurry.**

This is 2E's "hold your action" mechanism and the enabler for coordinated attacks (§4.7) and
for engaging flying opponents (§10.4).

### 4.6 Inactive (5/Special)

- Unconscious, paralyzed, helpless, or otherwise not choosing her own actions. **Not voluntarily
  chosen.** (p. 143)
- Enters **immediately** and aborts the pending action state when the condition arises (e.g.,
  being grabbed mid-interval).
- **Inactive characters cannot defend themselves; they start the action at DV 0.**
- Ends as abruptly as it begins. On the next available tick: act normally, refreshed DV, full
  options.

### 4.7 Miscellaneous action (5/Varies)

Anything that doesn't fit the other options. Remember a tick is ~1 second and Speed 5 gives about
five seconds of work — anything longer should be split into pieces. (p. 143)

**The DV penalty is the character's choice:**

| Focus | DV | Dice |
|---|---|---|
| Total concentration | **Forfeits all DV** (positive DV → 0; negative DV stays negative but is treated as 0) | full pool |
| One eye on the battle | **-1** (can vary) | **-2 dice** to the task |

Only the second variety can be part of a flurry (Storyteller discretion), in which case the
normal multiple-action dice penalty replaces the usual -2.

**Named miscellaneous actions:**

| Miscellaneous action | Speed | DV | Notes |
|---|---|---|---|
| **Join Battle (in progress)** | `reaction count − (Wits+Awareness successes)`, clamped to **[0, 6]** | **-0** | On Speed 0, "the character proceeds immediately to declare another action for that tick as if Join Battle was a reflexive action." Otherwise she waits until her next action. (p. 144) |
| **Coordinate attacks** | 5 | — | `Charisma + War`, difficulty = ⌊participants ÷ 2⌋. See below. (p. 144) |
| **Draw / ready weapons** | 5 | **-1** | As many weapons as she has hands. Diceless normally; extreme conditions → `Dexterity + combat Ability`, diff 1. Natural punch/kick needs no readying. (p. 144) |
| **Rise from prone** | 5 | **-1** | Prone imposes **-1 external penalty to all non-reflexive physical actions**. Normally automatic; `Dexterity + Athletics` under extreme conditions. (p. 144) |
| **Jump** | 5 | **-1** | **Only one jump per flurry / per action.** May move normally on the same tick. Short non-vaulting jumps within a normal move don't need declaring. (p. 144) |
| **Stanch bleeding (mortals)** | 5 | — | `Wits + Medicine`, difficulty = health levels in that injury. (p. 151) |
| **Reload** (Single Shot weapons) | 5 | — | Firewands, flame pieces, crossbows. (p. 372) |
| **Hide / re-establish surprise** | 5 | — | `Dexterity + Stealth` vs. all witnesses' reflexive `Wits + Awareness + 2`. May be flurried. (p. 156) |
| **Find a hidden character** | 5 | — | `Perception + Awareness` vs. the hider's Stealth successes. May be flurried. (p. 156) |

Note that Exalted **stanching bleeding is reflexive** (`Stamina + Resistance`, diff 2) on any tick
they may act — it is not a miscellaneous action for them. (p. 151)

**Coordinated attacks** (p. 144) are the most tick-relevant of these:

- Commander takes a Speed 5 miscellaneous action, rolls `Charisma + War` at difficulty
  ⌊group size ÷ 2⌋ (the commander counts himself if he wants to benefit).
- **Failure = the coordination fails entirely.** Don't overextend.
- Success opens a **"window of opportunity" on the tick when the commander next acts**.
- Every member who attacks the designated target **on that tick** reduces the target's DV by the
  **number of successes rolled**, capped at the number of attackers.
- Practically, participants must **Aim and/or Guard** so they can abort and attack on that tick.

This means the app needs a scheduled, tick-anchored, group-scoped DV modifier.

### 4.8 Move (0/None)

- **`Dexterity` yards per tick** over land, minus wound penalties, minus armor mobility penalty.
  **Floor of 1 yard/tick.** (p. 145)
- **Reflexive.** Does not require a roll unless the terrain is treacherous or slick.
- Swimming/climbing: **halve** base movement (round down), usually requiring a reflexive
  `Dexterity + Athletics`. Flying uses the listed flight rate.
- **The only restriction: a character can either move or dash on a given tick, but not both.**
- Because it is reflexive, it is available on **any tick, including ticks the character does not
  or cannot otherwise act.** (p. 142)

---

## 5. Charm timing types

Charms are the main source of nonstandard Speeds. Their **type** determines their timing. (p. 142)

| Type | Timing |
|---|---|
| **Permanent** | Auto-activates when learned; always on. |
| **Reflexive** | Activate on **any tick**, whether or not the character acts. May be used **multiple times on the same tick** if circumstances permit (e.g. a defensive Charm against every attack), but **never more than once in response to the same action or event**. |
| **Supplemental** | Only on a tick in which the character acts. Enhances an action. May be activated multiple times in a flurry, but **no more times than the number of actions in the flurry it can enhance**. |
| **Extra Action** | A magical flurry — a cascade of separate actions on a single tick, usually **without** multiple-action penalties. Only on a tick in which the character acts; **once per tick**. |
| **Simple** | **Constitutes an action by itself.** Sole action for that tick. **Default Speed 6** unless a Speed is listed in parentheses beside the type. Cannot be activated twice on a tick. |

**Cross-Charm exclusion rules** (these are real state machine constraints):

- **Reflexive:** a character may not use a reflexive Charm "unless they have not activated any
  other Charms since before the tick when they last took an action." Repeated use of the *same*
  reflexive Charm is fine. **Terrestrial Exalted are exempt** — they may activate any reflexive
  Charm at any time regardless of what else they've used.
- **Supplemental:** cannot be used if the character activates **any other** Charm during the tick.
- **Extra Action:** cannot be combined with any other Charm that tick, and the character may take
  **no non-reflexive actions except those granted by the Charm** (including no non-magical flurry
  actions).
- **Combos** are the mechanism for legally linking multiple Charms on the same tick (p. 244).

### 5.1 Sorcery — the canonical multi-action sequence (pp. 251–253)

Sorcery is the best worked example of a multi-tick activity and a good test case for the app.

| Action | Speed | DV |
|---|---|---|
| Shape Terrestrial Circle Sorcery | **5** | **-2** |
| Shape Celestial Circle Sorcery | **two actions, each Speed 5** | **-3** |
| Shape Solar Circle Sorcery | **three actions, each Speed 5** | **-4** |
| **Cast Sorcery** | **Varies — roll Join Battle to determine it** | **-0** |

While shaping, the sorcerer:

- cannot use Charms or Combos, **including reflexive Charms**;
- cannot take voluntary reflexive actions — **including speech, Move, or Dash**;
- *can* benefit from established ongoing/permanent Charms, and *can* activate his anima
  (explicit exception).

The sequence must be **unbroken**: Shape (×1/2/3) → Cast, consecutively, or the spell is
interrupted. Essence is committed for the duration of the shaping and released on the Cast.

**Interruption:** if distracted, reflexive `Wits + Occult` at difficulty 1, minus an external
penalty equal to health levels lost to the distraction. Failure dissipates the spell. Botch:
everyone within (spell's Circle) yards takes (sorcerer's Essence) dice of lethal "Essence burn."
**If the spell is lost, the character makes an immediate Join Battle roll** to re-enter combat.

The **Cast Sorcery Speed being a Join Battle roll** is unusual and worth calling out in the data
model: some action Speeds are rolled, not fixed.

---

## 6. Attack resolution — the ten steps

Verbatim structure from the "Order of Attack Events" box (p. 145), with detail from pp. 145–150.

1. **Declaration of Attack.** Attacker states the attack and declares all enhancing Charms
   (supplemental, extra action, simple, and any reflexive Charm that directly benefits the
   attack), *excepting* reroll effects. **If the attack is unblockable or undodgeable, that must
   be declared now.**
2. **Defender Declares Response.** Either accept the attack, or defend with the better of parry
   or dodge. Must declare defensive Charms not based on a reroll. Unless the defender opts
   otherwise, she **automatically falls back on whichever mode of defense has the better rating**.
3. **Attack Roll.** `Dexterity + (Archery | Martial Arts | Melee | Thrown)`, **difficulty 1**,
   subject to the standard order of modifiers (§7.4).
4. **Attack Reroll.** Reroll effects (e.g. Essence Resurgent). Each die rerolls at most once; best
   result is final. Cannot be used if another Excellency already augmented the attack. If roll and
   reroll both fail, the attack misses.
5. **Subtract External Penalties / Apply Special Defenses.** Apply external penalties, **with the
   defender's DV always last**. Roll stunt/Charm dice granted to defense and add successes to DV.
   Any other roll-based defensive effect resolves here (except rerolls). **No successes remaining
   = miss.**
6. **Defense Reroll.** Defender's reroll Charms.
7. **Calculate Raw Damage.** `base damage (usually Strength + weapon value) + successes remaining
   after step 5`.
8. **Apply Hardness and Soak, Roll Damage.** See §9.
9. **Counterattacks.** Resolved as a normal attack by the victim, steps 1–8. **Hits
   simultaneously** with the triggering attack. A counterattack cannot be counterattacked.
10. **Apply Results.** Damage, non-damage effects, and any counterattack's effects.

**Botched attacks:** additional **-2 DV** on top of the action's normal penalty, and the
Storyteller may rule the attack hits a bystander with "successes" equal to the number of 1s
rolled. (p. 148)

**Off-hand:** -1 die with a weapon in the off hand (not for unarmed maneuvers). (p. 148)

**Range penalties** (step 5, external): within Range = no penalty; up to 2× Range = **-1**;
2×–3× Range = **-2**; beyond 3× Range is impossible without magic. (p. 148)

---

## 7. Defense Values

### 7.1 Base values

| DV | Formula | Rounding |
|---|---|---|
| **Dodge DV** | `(Dexterity + Dodge + Essence[if Essence ≥ 2]) ÷ 2` | Exalted/divine **round up**; mortals and heroic mortals **round down** |
| **Parry DV** | `(Dexterity + wielding Ability + weapon Defense) ÷ 2` | same |

(p. 146)

Parry DV uses the Ability governing the currently equipped weapon with the **highest Defense**
— almost always Melee or Martial Arts.

Worked examples from the book:

- Mortal soldier, Dex 2 / Dodge 3 / Essence 1 → Dodge DV **2** (rounds down).
- Anoria (Solar), Dex 4 / Dodge 4 / Essence 5 → Dodge DV **7** (rounds up).
- Immaculate monk, Dex 3 / Martial Arts 4, fist (Defense +2) → Parry DV **5**. With a
  seven-section staff (Defense +3) he'd *still* be Parry DV 5, because the fist already rounded up.

**Characters cannot use a hand for parrying if they are holding a weapon in it** — so a sledge
(Defense -3) genuinely tanks your Parry DV.

### 7.2 Inapplicable defense

When a mode of defense is prohibited, that DV **drops to 0** — *and then bonuses and penalties
still apply on top of the zero* (so cover still helps). (p. 146)

- Choosing not to defend: **both** DVs = 0.
- Magically **unblockable**: Parry DV → 0 (Dodge unaffected).
- Magically **undodgeable**: Dodge DV → 0.
- **Unarmed characters — even Exalted — cannot parry lethal/aggravated attacks or ranged
  attacks**; Parry DV → 0 unless a stunt or magic enables it. Creatures with natural full-body
  armor are exempt.
- Dodge becomes inapplicable for characters **unable or unwilling to give ground** (close-ranked
  formations, narrow crevasses), or in terrain worse than -3.
- Magically declared unblockable/undodgeable **cannot** be circumvented by a stunt. Mundane
  inapplicability **can** be.

### 7.3 Modifiers

**Bonuses** (all cumulative): (p. 147)

| Type | Hand-to-Hand cover | Ranged cover |
|---|---|---|
| Buckler | +1 | None |
| Target Shield | +1 | +1 |
| Tower Shield | +1 | +2 |
| 25% hard cover (shoulder and leg) | None | +1 |
| 50% hard cover (half body) | +1 | +2 |
| 75% hard cover (all but shoulder, arm, face) | +1 | +3 |
| 90% hard cover (all but eyes) | +2 | +4 |

- **Shields and cover are not cumulative with each other** — only the greater bonus applies.
- **Height / elevation** (close combat only, negated by `reach`-tagged weapons):
  steps / gentle slope / mounted = **+1**; steep slope / howdah = **+2**; too steep to climb
  without hands / scaling a ladder = **+3**. Height and cover **do** stack for a mounted character
  on a slope. Both count as external penalties for the attacker.
- **Stunts** on defense — see §12.1 for a contradiction in the book here.
- Excellencies: Essence Overwhelming rolls dice → add successes; Essence Triumphant adds successes
  directly as DV; Essence Resurgent adds half the Ability to the derivation pool.

**Penalties** (all cumulative): (p. 147)

| Situation | Modifier |
|---|---|
| Taking actions | -(varies, per the action) |
| Wound penalties | -(1 to 4) — applies to **both** DVs |
| Wearing armor | -(mobility penalty) — **Dodge DV only** |
| **Onslaught** | **-1 per attack**, cumulative, **per attacker** |
| Coordinated attack | -(coordination successes), capped at attacker count |
| Unstable terrain | -1 (bad) to -3 (extreme); worse than -3 makes dodge inapplicable |
| Mounted/height disadvantage | -1 to -3 (mirror of the bonus above) |

**Onslaught penalty — exact rules** (p. 147):

> "If a character is attacked multiple times by the same opponent, each attack cumulatively
> imposes an additional -1 penalty to both DVs. […] Onslaught penalties apply **only when
> defending against the character that imposed them** and **only against the attacks of an
> individual flurry**. If an attacker acts a second time before the defender's DV refreshes, the
> **onslaught penalty is reset to 0** at the start of the second series of attacks."

So onslaught is a `(defender, attacker, flurry)`-scoped counter, reset per attacker action —
**not** per defender DV refresh. An automatic miss against an extra (below) still increments it.

### 7.4 Negative DV, automatic defense, and order of assembly

- **Negative DV:** treated as 0 for the attack (the enemy can still miss on zero successes), but
  **track the true negative value**, because DV-enhancing effects apply to the real number.
  (p. 147)
- **Automatic defense:** if a character's DV is higher than the **Accuracy dice pool** of an
  **extra's** attack, the attack **automatically misses without a roll**. It still counts toward
  imposing onslaught. (p. 148)
- **Assembly order:** establish base Dodge DV and Parry DV (0 if inapplicable) → add bonuses →
  apply penalties → **take the higher of the two** (unless the player chooses the worse one).
  (p. 148)

**Order of modifiers for dice pools** (p. 124), which the app needs for attack rolls:

1. Non-magical bonuses (stunts, specialties, equipment, Virtue channeling).
2. Non-magical penalties. *If Essence ≥ 2, record `wound penalties + multiple action penalties`
   for step 5.* May go to zero or negative.
3. Magical bonuses (Charms, spells).
4. Magical penalties. **If Essence 1, this is the final pool.**
5. **Minimum dice:** if Essence ≥ 2 and the modified pool is below Essence, the pool becomes
   `Essence − (wound penalties + multiple action penalties)`. Cannot exceed the original
   unmodified pool; may be zero or negative.
6. Bonus successes (Charms, Willpower, etc.). If none apply and dice ≤ 0, **automatic failure**.

---

## 8. Weapons — the attack data model

Every attack, whether from a weapon table or an NPC stat block, is described by the same tuple.
NPC stat blocks in Chapter Seven use the compact form:

```
Claw:   Speed 2, Accuracy 9, Damage 6L, Defense 1, Rate 2
Charge: Speed 6, Accuracy 7, Damage 10B, Defense 0, Rate 1
```

**Chart key** (p. 372):

| Field | Meaning |
|---|---|
| **Speed** | The Speed rating of the weapon = the Speed of an Attack action using it. |
| **Accuracy** | Added to `Dexterity + Ability` on attack rolls. *Clinch attacks may use Strength instead of Dexterity.* |
| **Damage** | Added to `Strength + successes`. Suffix `B`/`L`/`A` = bashing/lethal/aggravated. A slash (`+5L/2`) means the second number is the **innate minimum damage** (the `Overwhelming` tag), or the second Damage value for `Lance` type. |
| **Defense** | Added to `Dexterity + Ability` when parrying with it. |
| **Rate** | **Maximum number of attacks the weapon can make in a single flurry.** |
| **Minimums** | Required dots (`Str`, `Dex`, `Mrt`). |

**Minimums penalty (affects Speed!):**

> "For each dot the character is missing from any minimum, subtract one from the Accuracy and
> Defense of the weapon, and **add one to its Speed (to a maximum total of Speed rating 6)**.
> This penalty can cause a weapon's Accuracy and Defense to become negative and can worsen already
> negative values." (p. 372)

So effective weapon Speed is `min(6, base_speed + missing_dots)`. **Speed 6 appears to be the
hard ceiling throughout the system** (simple Charm default, First Action cap, Join-in-progress
cap, minimums cap).

**Tag key** (p. 372):

| Tag | Name | Effect |
|---|---|---|
| `2` | Two-Handed | Requires both hands. A character with **triple** the minimum Strength may wield it one-handed; short of that, **-1 external penalty per point** below the triple-Strength requirement. (p. 373) |
| `B` | Bow Type | Damage comes from the ammunition. |
| `C` | Clinch Enhancer | Must be used for clinching, wielded through Martial Arts. |
| `D` | Disarming | Additional **+2 Accuracy** when disarming. |
| `F` | Flame Type | Does not add Strength to damage; Range is the reach of the flame jet. |
| `L` | Lance Type | Damage increases to the second value when charging or bracing against a charge. |
| `M` | Martial Arts | May be wielded with Martial Arts **or** Melee. |
| `N` | Natural | Part of the body; not subject to disarming. **Must** use Martial Arts. **Can parry only bashing attacks** without a stunt or magic. |
| `O` | Overwhelming | The number after the slash is the weapon's innate minimum Damage (instead of the usual 1 die). |
| `P` | Piercing | **Halves the target's armored soak** (rounded down). |
| `R` | Reach | Can attack mounted or higher-elevation targets without penalty (and negates their height bonus). |
| `S` | Single Shot | Must use a **miscellaneous action to reload after every shot**. |
| `T` | Thrown | Also usable as a thrown weapon; see the thrown table for its stats. |

### 8.1 Unarmed (p. 372)

| Name | Speed | Acc | Damage | Defense | Rate | Minimums | Tags |
|---|---|---|---|---|---|---|---|
| **Punch** | 5 | +1 | +0B | +2 | 3 | Str • | N |
| **Kick** | 5 | +0 | +3B | -2 | 2 | Str •, Dex • | N |
| **Clinch** | 6 | +0 | +0B | — | 1 | Str • | C, N, P |

### 8.2 Melee (pp. 366–370) — OCR-limited

`?` = unreadable in the scan. Negative Defense values in *italics* had their minus sign restored
from context (the scan drops minus signs) — treat the magnitude as read and the sign as inferred.

| Weapon | Speed | Acc | Damage | Def | Rate | Tags |
|---|---|---|---|---|---|---|
| Chopping sword | 4 | +1 | +5L/2 | -1 | 2 | O |
| Great sword | 6 | +1 | +7L/2 | -1 | 2 | 2, O, R |
| Knife | ? | +1 | +0L | +0 | 3 | — |
| Short sword | 4 | +2 | +3L | +1 | 2 | — |
| Slashing sword | 4 | +1 | +3L | +0 | 3 | — |
| Straight sword | ? | +2 | +3L | +1 | 2 | — |
| Axe / hatchet | 4 | +1 | +5L | *-2* | 2 | T |
| Club / cudgel / baton | ? | +1 | +6B | +0 | 2 | T |
| Great axe | 6 | +1 | +7L/2 | *-2* | 2 | 2, O, R |
| Hammer | ? | -1 | +8B/2 | +1 | 2 | O, P |
| Mace | ? | +1 | +8B/2 | +1 | 2 | O, P |
| Poleaxe | 6 | +0 | +7L/2 | +0 | 2 | 2, O, R |
| Scythe | ? | +1 | +7L/2 | *-2* | 2 | 2, O, R |
| Sledge | 6 | -1 | +12B/4 | **-3** (confirmed in prose, p. 146) | 1 | 2, O, P, R |
| Staff | ? | +2 | +7B (`/3`?) | +2 | 2 | 2, R |
| Tetsubo | ? | -1 | +12B/4 | *-3* | 1 | 2, O, P, R |
| Whip | 5 | +1 | +1B | +0 | 2 | D, R |
| Short spear | 5 | +2 | +4L | +1 | 2 | R |
| Spear | ? | ? | ? (lance: two values) | ? | ? | L, R |
| Hook sword | 5 | +0 | +3L | +3 | 3 | D, M |
| Sai | 5 | +0 | +2L | +2 | 3 | D, M |
| Sai (item clinched) | 6 | +2 | +1B | — | 1 | C, D, R |
| Seven-section staff | 5 | -2 | +7B | +3 | 2 | M |
| Wind-fire wheel / war fan | 5 | +2 | +1L | +2 | 3 | M |
| Iron boots | 5 | +0 | +6B | *-3* | 2 | M |
| Razor harness | 6 | -1 | +3L | — | 1 | ? |
| Khatar (punch dagger) | ? | +0 | ? | ? | 3 | M |
| Cestus | ? | ? | ? | ? | ? | M |
| Fighting gauntlet | ? | ? | ? | ? | ? | M |

The **staff** row is the one real puzzle: the scan reads `+73`, which is either `+7B` or `+7B/3`.
The `/3` form would require the `O` (Overwhelming) tag, and the scanned tag list is `2,R` with no
`O` — so `+7B` is the likelier reading, but confirm it.

Notes attached to specific weapons in the prose (reliable, not OCR-dependent):

- **Iron boots**, **cestus**, and **khatar** cannot block lethal attacks without a stunt.
- **Fighting gauntlets** *can* block lethal attacks with Martial Arts ("though they slow the
  character's blows" — i.e. the Speed is deliberately high).
- **Hook swords** and **wind-fire wheels** are always wielded paired.
- **Whips** used for a clinch must use Martial Arts. A flexible rod or cane is a whip without the
  disarming, reach, and clinch enhancements.
- **Spear**: jabbing uses the lower Damage value; charging or bracing against a charge uses the
  higher (the `L` lance tag). Short spears get no charge/brace bonus.

Sample **artifact** Speeds, from the book's own tick-tracking illustration (p. 140):
Reaper Daiklave **4**, God-Kicking Boot **5**, Grimscythe **6**, Serpent-Sting Staff **5**.

### 8.3 Thrown (pp. 370–371)

| Weapon | Speed | Acc | Damage | Rate | Range |
|---|---|---|---|---|---|
| Axe / hatchet | 5 | +0 | +3L | 2 | 10 |
| Chakram / shuriken | 4 | +0 | +1L | 3 | 20 |
| Club / cudgel / baton | 5 | +0 | +3B | 2 | 10 |
| Javelin | 4 | +1 | +3L | 2 | 30 |
| Knife | 5 | +0 | +2L | 3 | 15 |
| Needle | 5 | -1 | 1L | 3 | 10 |
| Sling | 5 | -1 | +2L | 1 | 100 |
| War boomerang | 5 | +0 | +3L | 2 | 20 |

### 8.4 Archery (pp. 371–372)

| Weapon | Speed | Acc | Damage | Rate | Range | Notes |
|---|---|---|---|---|---|---|
| Composite bow | 6 | +0 | (ammo) | 3 | 250 | Max Str 5; `2, B` |
| Long bow | ? | ? | (ammo) | ? | ? | Damage caps at Strength 4 |
| Self bow | ? | ? | (ammo) | ? | ? | Damage caps at Strength 3 |
| Crossbow | ? | ? | (ammo) | ? | ? | Fowling or target bolts only; exceptional versions cannot raise Rate |
| Firewand | ? | ? | — | ? | 10 max | `F, S` — reload as a miscellaneous action |
| Flame piece | ? | ? | — | ? | 8 max | `F, S` — dual-wielding raises Rate to 2 |

**Ammunition** determines bow damage (p. 371):

| Arrow | Damage | Effect |
|---|---|---|
| Broadhead | Str +2L | — |
| Frog crotch | Str +4L | Target's **armor lethal soak is doubled** |
| Target | Str +0L | **Piercing** — armor lethal soak halved (round down) |
| Fowling | Str +2B | Bashing |

### 8.5 Equipment quality (p. 367)

| Quality | Weapons | Armor |
|---|---|---|
| **Fine** | +1 to one of: Accuracy, Damage, Defense, Range (+50 bows / +10 thrown), or reduce one trait minimum by 1 | +1L/1B soak |
| **Exceptional** | Three +1s across those characteristics **plus Rate**; no characteristic raised twice | Two +1s across soak (+1L/1B), mobility (-1 penalty), fatigue (-1) |
| **Perfect** | Two +1s and one +2 across the exceptional set; **Rate cannot take the +2** | Mobility and fatigue each -1, plus two +1s from the exceptional set |

Note that **quality never modifies Speed** — only the Minimums penalty does.

### 8.6 Armor (pp. 373–374)

Three stats that matter to combat:

- **Soak** `+XL/YB` — added to natural soak.
- **Mobility penalty** — an **internal** penalty. Subtracts from **Dodge DV**, Athletics rolls
  for whole-body feats, and movement/dash rates. **Does not normally apply to attacks or
  parries.** Always applies to swimming.
- **Fatigue value** — `Stamina + Resistance` at that difficulty; failure = a cumulative **-1
  internal penalty to all actions**. Rolled every 4 hours of normal activity, halved per factor
  (heat, sun, exertion), doubled for shade/cold/rest; max 8 hours between checks. Recovers at 1
  point per 8 hours of rest out of armor.

Examples: buff jacket `+3L/4B`, mobility -1, fatigue 2; chain shirt `+3L/1B`, mobility 0,
fatigue 1; chain hauberk `+6L/7B`, mobility -3, fatigue 2.

**Donning armor** takes minutes equal to the mobility penalty (a -0 penalty = 30 seconds); half
that if rushed (botch risk). Removal is half the donning time.

---

## 9. Damage

### 9.1 Damage types (pp. 148–149)

| Type | Natural soak | Below Incapacitated | Notes |
|---|---|---|---|
| **Bashing** | **Stamina** (everyone) | Passes out | Default for human unarmed attacks. Heals 1 level / 12h rest (mortals), 1 / 3h (Exalted). |
| **Lethal** | **0** for mortals; **⌊Stamina ÷ 2⌋** for Exalted/spirits | Starts **dying** | Heals at a rate dependent on the level's wound penalty. |
| **Aggravated** | **None for anyone** | Starts dying | **Armor's aggravated soak = its lethal soak.** Heals at the lethal rate. |

### 9.2 Steps 7–8: raw damage, Hardness, soak

1. **Raw damage** = `base damage (usually Strength + weapon value) + successes remaining from
   step 5`.
2. **Hardness:** if `raw damage ≤ Hardness`, the attack is **utterly ineffective** — no damage at
   all. Otherwise Hardness is ignored entirely. Only the highest Hardness per damage type applies.
   (e.g. Invulnerable Skin of Bronze = Hardness 6L/12B.)
3. **Soak** = natural soak + armored soak. **Piercing halves armored soak only** (round down);
   natural soak is never pierced.
4. **Minimum damage:** if `raw − soak` is less than the attack's innate minimum damage (from the
   `Overwhelming` tag, default 1) **or the attacker's permanent Essence**, final damage = the
   **greater** of those two. Capped at the original raw damage — "Essence can overcome soak, but
   it cannot generate damage where it does not exist."
5. **Roll** dice equal to final damage. Successes = health levels. **Cannot be botched; 10s do
   not count double.**

### 9.3 Health track (p. 149 and Chapter Seven stat blocks)

Standard mortal/Exalted track: **`-0 / -1 / -1 / -2 / -2 / -4 / Incapacitated`** (7 levels).

Larger creatures simply have longer tracks in the same shape, e.g. a dog of the unbroken earth is
`-0/-1/-1/-1/-2/-2/-2/-4/Incap` and Gri-Fel is
`-0/-0/-1/-1/-1/-1/-1/-1/-2/-2/-2/-2/-2/-2/-4/Incap`.

- Wound penalty = the penalty tier of the **lowest filled** level. It is an **internal penalty**
  and subtracts from **both DVs** and from movement rates.
- Damage fills from the **top down**. **More serious damage displaces lesser damage**, pushing
  minor injuries downward. Marks: `/` bashing, `X` lethal, `*` aggravated (`\` converts bashing
  to lethal, `|` lethal to aggravated).
- **Dying levels:** once Incapacitated fills with lethal/aggravated, the character has **Stamina**
  Dying levels. Each combat action interval while dying, the character is **inactive and takes
  one additional level of unsoakable lethal damage**. Out of Dying levels = dead.
- Stabilizing: `Wits + Medicine` at difficulty `5 + Dying levels filled`; success heals all Dying
  levels, **failure kills the patient immediately**. Magical healing stabilizes automatically.

### 9.4 Post-damage effects (pp. 154–155) — all tick-relevant

| Effect | Trigger | Resolution |
|---|---|---|
| **Knockdown** | raw damage > defender's `Stamina + Resistance` | Reflexive `(Dex or Sta) + (Athletics or Resistance)`, **difficulty 2**. Failure = prone. **Rising from prone costs an action** (Speed 5, -1 DV) and prone is -1 external to all non-reflexive physical actions. |
| **Tackling** | deliberate tackle connects | Immediate knockdown check for **both** parties; the target is **stunned even if the roll succeeds**. |
| **Sweeping** | chain/kick/staff sweep | **-2 Accuracy**; on a hit (damage or not) the victim checks for knockdown. |
| **Knockback** (cinematic alternative) | — | 1 yard per **3 dice of raw damage**, landing prone. Never adds damage. |
| **Stunned** | actual health levels taken > Stamina | Reflexive `Stamina + Resistance` at difficulty `(damage − Stamina)`. Failure = **-2 dice to all non-reflexive rolls until the tick the attacker next acts**. |
| **Bleeding** | any lethal/aggravated damage | 1 unsoakable lethal level per `Stamina` minutes until stanched. Exalted stanch reflexively (`Sta + Res`, diff 2) on any tick they may act; mortals need a Speed 5 `Wits + Medicine` roll at difficulty = levels in that injury. |

Note the **stunned duration is expressed in the attacker's action cycle**, not the victim's — a
distinct kind of timer from DV refresh.

### 9.5 Inanimate targets (p. 153)

- No DV (barring exceptions), and **no minimum damage** — so effectively `Hardness = soak`.
- **Damage is not rolled**: every die past soak inflicts one level.
- No wound penalties. Health expressed as `Damaged / Destroyed`.
- Samples: house door 1/3 soak, 3/10 HL; oak door 3/5, 10/20; fortress gate 8/10, 20/40; wood wall
  3/5, 8/12; brick wall 6/10, 24/40; stone wall 12/18, 40/80; wood statue 2/4, 3/16; stone statue
  4/8, 4/28; iron statue 6/12, 6/50.

### 9.6 Extras (p. 156)

The "minion" template — the app should treat this as a flag on a combatant:

- **Three health levels only:** Unhurt, **-1**, **-3**, Incapacitated, Dead. The -3 level counts as
  -4 for impaired movement.
- Usually **die immediately** when lethal damage takes them below Incapacitated (no Dying levels).
- **Damage is not rolled:** `⌈(raw − soak) ÷ 3⌉` levels. Any hit does at least one level.
- **Not heroic:** 10s do not count double, normally no stunts, no Willpower for bonus successes or
  Virtue channeling (except in defense of Motivation).
- Subject to **automatic defense** (§7.4): a DV higher than the extra's Accuracy pool = automatic
  miss, no roll.

---

## 10. Battlefield state

### 10.1 Multiple opponents (p. 155)

- **Max 5** human-sized attackers in close combat against a human-sized target in open terrain;
  **max 3** in cramped quarters (hallway, stairwell, doorway) or fewer. Scales with attacker size:
  more for smaller creatures, **at most one per side** for large ones.
- Anyone pressed inside a maximum cluster **cannot choose to move or dash away** and suffers
  **-2 Dodge DV** unless a stunt or magic lets her evade without giving ground.
- Worse: if she cannot maneuver, **one of her opponents gains the benefits of an unexpected
  attack**. The *defender's* player chooses which opponent she exposes her back to.
- **No limit** on ranged attackers.

### 10.2 Unexpected attacks (pp. 155–156)

- Unperceived attacks are **unblockable and undodgeable**: both DVs are 0 (cover can still improve
  them). Magic that detects surprise or defends against unperceived attacks is the only out.
- **Ambush roll:** attacker's `Dexterity + Stealth` vs. victim's `Wits + Awareness`.
  - Victim distracted, or attacker entirely outside her senses (e.g. directly behind): **-2
    internal penalty** to the victim.
  - Victim actively wary: **+1 die**.
  - **Ambush from plain view** (assassin at a banquet): **+2 difficulty** to the Stealth roll, and
    the scene must not already be in combat.
  - Resolve this **immediately before** the Join Battle roll.
- **Re-establishing surprise mid-fight:** a Speed 5 miscellaneous action (flurryable). Attacker's
  `Dexterity + Stealth` vs. **independent reflexive** `Wits + Awareness + 2` for each witness.
  Invisibility or similar = **+2 automatic successes**. Any witness who loses **loses track of
  the character entirely** and cannot attack or interact with him until he reveals himself or a
  third party calls out his location. Attacking gives away his location again.
- Melee Charms require a **ready weapon**, so parry-based defenders are at a real disadvantage
  against ambush unless they drew steel in advance.

### 10.3 Grappling / clinch (pp. 156–157)

This is the biggest special case in the tick system — it hijacks both characters' action cycles.

**The clinch attack:** `(Strength or Dexterity) + Martial Arts`. **Speed 6, Accuracy +0, Rate 1.**
Can be dodged or parried normally. Inflicts **no damage** on the initial hit. May be part of a
flurry.

On a hit, the attacker **controls** the clinch and **the victim's action shifts immediately to
inactive**. The controller may:

| Task | Effect |
|---|---|
| **Break Hold** | Throw the opponent `Strength` yards back (knockback + immediate knockdown check), or slam him to the ground (automatically prone), or simply release. |
| **Crush** | Bashing damage = `Strength + remaining successes on the clinch roll`. **This is a piercing attack.** |
| **Hold** | Pin motionless, no injury. |

**Maintaining a clinch:**

- The controller **must use every subsequent action to renew the clinch** and can do nothing else
  without flurrying.
- **Without a stunt or magic, the controller cannot block or dodge.** The held character cannot
  either (inactive), though she may use reflexive Charms and Charms designed to work in a clinch.
- **Renewal:** reroll the aggressor's `(Str or Dex) + Martial Arts`, **reflexively resisted** by
  the victim's same pool. The winner controls the clinch and picks a task, **adding net successes
  to damage if crushing**.
- **Reversal:** "If a character held in a clinch turns the tables on his opponent, then his action
  immediately switches to attacking and the former aggressor switches to inactive, **resetting the
  appropriate speed of each from that tick**."

**Dogpiling:** limited teamwork — each helper adds **+1 die** to the lead aggressor's clinch roll.
A helper joining an already-clinched victim needs only **one success** on her attack roll, then
contributes her die without rolling. If the victim breaks free, he breaks free of **everyone
except the leader**, whom he may damage, throw, or hold normally.

### 10.4 Terrain and mounts

- **Flight** (p. 153): ground-based opponents can attack a flier in close combat **only on the
  actual tick the flier dives to attack**. The target of a fly-by attacks at no penalty; allies
  in reach are at **-2** (**-1** with spears/long weapons). "As a rule, defenders must assume a
  **guard** stance, waiting to abort with an attack" — a direct, concrete use of Guard's abort.
- **Mounted** (p. 154): **all attack pools and DVs use the lower of the relevant Ability or the
  character's Ride rating**. Every mount has a **control rating** (0 automata/undead → 3 horse →
  6+ tyrant lizard). If Ride < control rating, the character must either take a Ride miscellaneous
  action or **flurry** the Ride roll with anything else she wants to do; a failed Ride roll
  **cancels the rest of the flurry**. Mounted = +1 DV vs. close combat; in a howdah = +2 and only
  `reach` weapons can reach them.
- **Unstable footing** (p. 155): same structure, with **Athletics** replacing Ride and an
  **instability rating** replacing control rating. Failure = no other flurry actions; botch (or
  failure when difficulty is twice the successes rolled) = prone or worse. **Being struck forces a
  new Athletics check**, but that one is reflexive (full pool). Instability is cumulative:
  slickness +1..+3, narrowness +1..+4, wind +1..+3, moving ground +1..+3.
- **Water/muck** (p. 155): ankle-deep = no penalty; knee-deep liquid or mid-calf mud = **-1
  external, half speed**; waist-deep or knee-deep mud = **-2 external, quarter speed**. Swimming
  avoids these but may make the water unstable terrain, in which case Athletics caps combat
  Abilities and the character must flurry to swim while doing anything else. Underwater: **-2
  external** to Dexterity/movement rolls for non-aquatic characters, and bows and wide bludgeons
  simply don't work.

### 10.5 Morale (p. 157)

Valor check when facing a perceived threat, re-rolled whenever the perception of threat changes.
Difficulty 1 (equal) → 5 (seems invincible; Celestial Exalted). Success = no penalty; **failure =
-2 internal penalty from fear**; botch = flees or cowers. **Valor > difficulty = automatic
success.** Non-extras may substitute another appropriate Virtue. **The Exalted are exempt from
Morale checks entirely** — only magic can rout them.

---

## 11. Long-tick modes

Two other systems reuse the identical Speed/DV/refresh machinery on a **one-minute "long tick"**.
If the app models the tick loop generically, both come nearly free.

### 11.1 Mass combat (pp. 158+)

- Units, not individuals. **Long ticks ≈ one minute each.** (p. 158)
- **Join War** replaces Join Battle: first action uses `(Wits + War) − Magnitude` for units;
  solo units and independently-acting heroes use `Wits + Awareness` as normal. (p. 162)
- Unit concealment: `(Dexterity + Stealth) − Magnitude`, opposed by the best
  `Perception + Awareness` among the enemy's commander and special characters.
- **Movement multiplier by formation** — units move at X× their normal rate per long tick:
  solo ×100, skirmish ×100, relaxed ×70, close ×40, unordered ×30. (p. 162)
- Mass-combat miscellaneous actions with explicit Speeds (pp. 163–164):

| Action | Speed | DV |
|---|---|---|
| Change Formation | 5 | -1 |
| **Disengage** | **0** | -0 (reflexive) |
| Turn (>90°) | 3 | -1 |
| Split Unit | 3 | -1 |
| Expel a single special character | **0** | -0 (reflexive) |
| Merge Units | 3 | -1 |
| Signal Units | 3 | -0 |
| Rally | 4 | -1 |

- A unit can attack **only one other unit per action**, even with a flurry — but flurrying the
  *same* unit repeatedly is effective.
- **Special characters** are not tracked separately until they act independently. A player may
  Join War for a fast special character as if she were a solo unit; otherwise she cannot act
  independently until a long tick when her unit acts. Once acting separately she stays "out of
  sync" until she uses **Guard to wait and realign** ("falling back in line"). (p. 166)
- Charms: characters "substitute long ticks for standard ticks," and **may use any reflexive Charm
  at any point in any long tick** regardless of what else they used.

### 11.2 Social combat (pp. 169+)

- **Join Debate** replaces Join Battle, using the same `Wits + Awareness` roll. Time advances in
  **long ticks of one minute**, the same scale as mass combat. (p. 169)
- Surprise = an unexpected shift in conversation: `Manipulation + Socialize` vs. reflexive
  `Perception + Investigation`. Re-establishing surprise requires **changing the subject**.
- Action list (p. 171):

| Action | Speed | DV |
|---|---|---|
| Move | 0 | -0 |
| Dash | 3 | -3 |
| Guard | 3 | -0 |
| Inactive | 3 | Special |
| **Monologue / Study** (the social Aim) | 3 | -2 |
| Miscellaneous Action | 5 | -2 |
| Join Debate | 5 | **-0** |
| Read Motivation | 5 long ticks | — |
| Flurry | Varies | Varies |
| Activate Charm/Combo/Power | Varies | Varies |
| Attack | Varies (by Ability) | -2 |

- **Attack Speeds are set by the Ability used** (pp. 171–172):
  **Presence** Speed 4, Rate 2 · **Investigation** Speed 5, Rate 2 · **Performance** Speed 6,
  Rate 1. Presence and Investigation reach one target (an individual *or* one organized social
  unit); Performance reaches **everyone who can perceive it, with no ability to exclude anyone**.
- Unlike physical combat, a **fully-concentrating** miscellaneous action does not zero MDV —
  it grants **social invulnerability**, like being inactive. (But a character covering his ears
  automatically suffers surprise penalties if the other party gives up talking and attacks.)
- **Mental Defense Values** (p. 172):
  - **Dodge MDV** = `⌊(Willpower + Integrity + specialty + Essence) ÷ 2⌋`
  - **Parry MDV** = `⌈((Charisma or Manipulation) + Ability + specialty) ÷ 2⌉`
- MDV takes DV penalties for actions, **onslaught**, and **coordinated attacks** (via Socialize
  rather than War) exactly as physical DV does, and refreshes the same way. Physical modifiers
  (cover, terrain, reach) do **not** apply. Two extra modifiers do:
  - **Relative Appearance:** `(defender's Appearance − attacker's Appearance)`, **clamped to
    ±3**. Via letters/books, effective Appearance = the writer's **Linguistics**.
  - **Intimacy / Virtue / Motivation** modifiers when the attack pushes against them.
- To physically attack, a character must take a **Join Battle** action, which drops the whole
  scene out of social combat and back into standard ticks.

---

## 12. Known ambiguities and decisions the app must make

### 12.1 Defensive stunts: rolled, or added flat?

The book contradicts itself.

- **Chapter Four, p. 147 (combat):** "Dice awarded for stunts can temporarily inflate one of the
  two DVs against a specific attack, in which case the **defender's player rolls the stunt dice
  separately and adds any successes** to the character's DV."
- **Chapter Four, p. 126 (traits/stunts):** "Stunts can be used to enhance static values like DV.
  When stunts aid a static value, treat the bonus dice as if they had been awarded by successes
  rolled with the First Excellency. That is, **add the stunt bonus directly to the character's
  DV, without dividing by two.** Thus, a character with a Parry DV of 4 who described his defense
  spectacularly and received the three-die stunt bonus would have a DV of 7."

The p. 126 text is the later, more explicit statement and matches published errata. **Recommend
implementing the flat bonus**, with a setting for the rolled variant.

Related: "Characters don't have to stunt their dodge for every attack in a flurry. Just have them
make one stunt out of their defensive antics and apply the bonus to the DV before the first
attack." (p. 126)

### 12.2 Stunt rewards (p. 126)

- **1-die** stunt: good description; may perform borderline-impossible feats.
- **2-die** stunt: interacts notably with the environment; grants **limited dramatic editing**.
- **3-die** stunt: singular greatness. "If any doubt exists as to whether a stunt merits three
  dice, it isn't a three-die stunt." Same editing rights as 2-die.
- **Reward on success:** motes equal to **2× the stunt rating**. For 2- and 3-die stunts, the
  player may take **one Willpower instead**. Optionally, one XP for a natural 3-die stunt.
- **Heroic mortals** reduce both bonus and reward by one category (2-die → 1 die, 3-die → 2 dice).
  Without an Essence pool they regain no motes, but may recover one Willpower from a 3-die stunt.
- Important ST characters may stunt sparingly; **extras never stunt**.

### 12.3 Speed 6 as a ceiling

Speed 6 shows up as an explicit cap in four places: First Action, Join-in-progress Speed, the
Minimums penalty, and the default Speed of Simple Charms. Nothing in the core rules states a
global "no action exceeds Speed 6," but every capped mechanic caps there. Treat 6 as the
conventional maximum and 0 as the minimum for individual-scale actions.

### 12.4 Errata not in this corpus

The Exalted 2E core book had substantial published errata, and the 2.5 revision changed combat
significantly (notably around perfect defenses and Charm costs). **None of that is in the local
corpus** — everything above is core-book-as-printed. If the app targets 2.5 or errata'd 2E,
those documents need to be sourced separately.

---

## 13. Data model checklists

### 13.1 To start a new battle, collect:

**Scene:**

- [ ] **Reaction count** (highest Join Battle successes among simultaneous joiners) — a scene
      constant, needed later for characters joining in progress
- [ ] Whether an ambush/surprise check preceded Join Battle, and who won it
- [ ] Terrain: instability rating (slickness + narrowness + wind + moving ground)
- [ ] Terrain: Dodge DV penalty (-1 bad to -3 extreme), and whether dodging is inapplicable
      (close ranks, crevasse)
- [ ] Cover available per combatant (25/50/75/90%) and elevation/slope relationships
- [ ] Water/muck depth
- [ ] Whether the space is open (5 close-combat attackers max) or cramped (3)
- [ ] Lighting/visibility penalties

**Per combatant:**

- [ ] Name, side, and **is-extra flag**
- [ ] Attributes: Str, Dex, Sta; Wits (Join Battle); Cha/Man/App (social)
- [ ] Essence rating (affects DV rounding, minimum dice pools, and minimum damage) and Essence pool
- [ ] Willpower (current/permanent), Virtues (Valor for morale)
- [ ] Abilities: Dodge, Melee, Martial Arts, Archery, Thrown, Athletics, Resistance, Awareness,
      Ride, Integrity, War, Presence/Performance/Investigation
- [ ] **Join Battle roll result** → **First Action tick** = `clamp(reaction count − successes, 0, 6)`
      (botch ⇒ 6)
- [ ] Base **Dodge DV** and **Parry DV** (with correct rounding direction for the character type)
- [ ] **Soak**: natural (Sta bashing / ⌊Sta÷2⌋ lethal for Exalted, 0 for mortals) + armor L/B,
      and any **Hardness**
- [ ] **Health track** shape and current damage by type
- [ ] Equipped weapons, each with `Speed / Accuracy / Damage / Defense / Rate / Range / tags`, plus
      whether trait minimums are met (which raises Speed)
- [ ] Armor mobility penalty and fatigue value; accumulated fatigue penalties
- [ ] Shield type, if any
- [ ] Ride rating and mount control rating, if mounted
- [ ] Off-hand weapon flag
- [ ] Known Charms with their **type** (permanent/reflexive/supplemental/extra action/simple),
      Speed, and cost; known Combos

### 13.2 Per-combatant runtime state the tick loop must carry:

- [ ] `next_action_tick`
- [ ] Current DV penalty from the last action, and whether the next action **refreshes DV**
      (false after aborting Guard or Aim)
- [ ] **Onslaught counters**, keyed `(defender, attacker)`, reset at the start of each of that
      attacker's action series
- [ ] Action state: normal / **guarding** / **aiming(target, banked_dice)** / **inactive** /
      **clinching(controller, victim)** / **shaping sorcery(circle, actions_remaining)** / dying
- [ ] Prone flag (-1 external to non-reflexive physical actions)
- [ ] Stunned flag with expiry expressed as **the attacker's next action tick**
- [ ] Bleeding wounds with their per-`Stamina`-minutes timer
- [ ] Scheduled coordinated-attack windows: `(tick, target, DV penalty, participant set)`
- [ ] Charms activated this tick and since the last action (for the reflexive/supplemental/extra
      action exclusion rules); Terrestrial Exalted exempt from the reflexive rule
- [ ] Committed Essence
- [ ] Whether a move or dash has been used this tick (mutually exclusive)

### 13.3 To add a new activity/action to the system, you need:

**Required:**

1. **Speed** — a fixed integer, a formula (`reaction count − successes`), or "roll for it"
   (Cast Sorcery). Note whether it is a **weapon Speed** (variable per equipment) or intrinsic.
2. **DV penalty** — a fixed value, a range, or a player choice (miscellaneous actions).
3. **Reflexive?** — reflexive means Speed 0, **no DV refresh**, doesn't count as a "true action"
   for effects that last until the next action, and is usable on ticks the character can't act.
4. **Does it refresh DV?** — the default is yes for non-reflexive actions; Guard-abort and
   Aim-abort are the exceptions.
5. **Flurryable?** — and if yes, whether there is a cap (weapon Rate, one jump per flurry).
6. **Dice pool and difficulty** — or "diceless" / "automatic."

**Frequently needed:**

7. **Target/scope** — self, single target, group, area, or a whole unit.
8. **Rate** — max repetitions in a flurry.
9. **Prerequisites** — armed vs. unarmed, hands free, ready weapon, a minimum Ability, not
   clinched, not inactive.
10. **State it enters or exits** — Guard, Aim, clinch, inactive, shaping, prone.
11. **Multi-action sequences** — does it require *N* consecutive actions (Celestial/Solar
    sorcery), and what breaks the sequence?
12. **Duration of effects** — 2E expresses these in at least four different ways, all of which
    the app must represent distinctly:
    - "until the character's next action" (DV penalties)
    - "until the **attacker's** next action" (stunned)
    - "on the tick the commander next acts" (coordinated attack window)
    - real-time intervals (bleeding, per `Stamina` minutes)
13. **Interaction with Charm exclusion rules** — does taking it count as activating a Charm?
14. **Abort behavior** — can it be aborted, into what, and what happens to Speed and DV?

---

## 14. Quick reference: every Speed value in the core rules

| Speed | Actions |
|---|---|
| **0** | Move; reflexive Charms; Join Battle in progress (when successes ≥ reaction count); mass-combat Disengage; expelling a special character; social Move |
| **2** | (weapon-specific: e.g. a claw natural attack) |
| **3** | Aim; Dash; Guard; social Monologue/Study; social Guard; social Inactive; social Dash; mass-combat Turn/Split/Merge/Signal |
| **4** | Chopping/short/slashing sword; axe; chakram; javelin (thrown); social Presence attack; mass-combat Rally |
| **5** | Miscellaneous actions (all of them); Inactive; punch; kick; whip; short spear; most thrown weapons; Shape Sorcery (each action, all Circles); social Investigation attack; social miscellaneous; mass-combat Change Formation |
| **6** | Great sword; great axe; poleaxe; sledge; tetsubo; composite bow; **clinch**; **default Simple Charm**; social Performance attack; the ceiling for First Action, Join-in-progress, and the Minimums penalty |
| **Varies** | Attack (= weapon Speed); Flurry (= highest Speed in the cascade); Activate Charm (= Charm type/listing); Cast Sorcery (= a Join Battle roll) |
