# Munchkin — Base Game Rules

A precise, implementation-oriented reference for the **base Munchkin** card game
(no expansions). Distilled from the official rulesheet (as reproduced in *The
Munchkin Bible*, © Mick 2014) and cross-checked against the actual card text in
[`cards.toml`](./cards.toml).

The goal of this document is to capture every rule needed to *run* a game
programmatically: turn flow, combat resolution, card timing, and all the
edge-case rulings. Flavor and humor from the original have been dropped; rules
content has not.

> **Scope note.** This covers only the base set. Rules for Hirelings are
> included because the base set contains one Hireling card (it is a *Door* card
> in this set). Expansion-only concepts (Steeds, Dungeons, Portals, Epic play,
> Sidekicks/Mooks/Minions, multi-set deck building, etc.) are intentionally
> omitted.

---

## 1. Components & Concepts

- **Players:** 3 to 6.
- **Level counters:** Each player needs a way to track a number from 1 to 10
  (10 tokens, a die, an app, etc.). Level never exceeds 10 (reaching 10 ends the
  game) and never drops below 1.
- **Two decks:**
  - **Door deck** — Monsters, Curses, Races, Classes, Monster Enhancers,
    Hirelings, and other "Door"-type specials.
  - **Treasure deck** — Items, "Go Up a Level" cards, and other "Treasure"
    specials.
- **Two face-up discard piles**, one per deck. Players may **not** look through
  a discard pile unless a card explicitly lets them.

### Deck reshuffling
When a deck runs out, **reshuffle its discard pile** to form a new draw pile.
If a deck runs out *and* its discard pile is empty, **no one can draw that type
of card** until cards return to that discard pile.

### Card location states
A card is in exactly one of these states:

- **In hand** — Cards held by a player. They are *not* "in play"; they provide
  no benefit and do not represent what they say (a Race card in hand is just a
  card, not a Race). Cards in hand cannot be taken except by effects that
  specifically target "your hand."
- **In play** (on the table in front of a player) — Active Race, Class, carried
  Items, continuing Curses, and other persistent cards. These represent what
  they say they are.
- **In the discard pile.**
- **In a draw deck.**

Cards in play may **not** be returned to hand. To get rid of an in-play card you
must discard it (only via a legal channel — see §10.4) or trade it (Items only).

---

## 2. Setup

1. Separate cards into the **Door deck** and **Treasure deck**; shuffle each.
2. Deal **4 cards from each deck** to every player (8 cards total per player).
3. Everyone starts as a **Level 1 Human with no Class**.
4. **Opening play:** Each player looks at their 8 cards and may immediately put
   into play, in front of them:
   - one Race card (and a Half-Breed card if they have one and a Race),
   - one Class card (and a Super Munchkin card if they have one and a Class),
   - any usable Items.
   (See §7 for what's legal to play. Special "play immediately" cards in the
   opening hand, e.g. *Hoard!*, *Divine Intervention*, trigger at this point.)
5. Decide who goes first by any means (e.g. roll a die). Play proceeds clockwise
   (to the left).

---

## 3. Character Stats

A character is a collection of Items plus three stats: **Level, Race, Class**
(and an implicit **Sex**).

### 3.1 Level
A measure of power. When rules/cards say "**Level**" (capitalized) they mean
this number.
- **Gain** a level by killing a monster, or when a card says so (e.g. "Go Up a
  Level" cards), or by selling Items (§9.4).
- **Lose** a level only when a card says so.
- **Level can never drop below 1.** (Combat strength *can* go negative — see
  §5.1.)
- **You must kill a monster to reach Level 10** (the winning level). You may not
  reach Level 10 by selling Items, by "Go Up a Level" cards, or any other
  non-combat means unless a card *explicitly* says it lets you win.

### 3.2 Race
Choices: **Human, Elf, Dwarf, Halfling**. With no Race card in play, you are
**Human** (Humans have no special abilities).
- You gain a Race's abilities the moment you play its card, and lose them the
  moment you discard it. Discarding your Race card makes you Human again. You
  may discard a Race card **at any time, even mid-combat.**
- You may not hold more than one Race at once unless you play **Half-Breed**.
- You may not have two copies of the same Race card in play.
- Some Race abilities are powered by **discards**: you may discard any card (from
  hand or in play) to power such an ability.

Base-set Race abilities (actual card text):
| Race | Abilities |
| --- | --- |
| **Human** | None. |
| **Elf** | +1 to Run Away. You go up 1 Level for every monster you help someone else kill. |
| **Dwarf** | You can carry any number of Big items. You can have 6 cards in your hand (instead of 5). |
| **Halfling** | −1 to Run Away. You may sell one item each turn for double price (other items at normal price). |

### 3.3 Class
Choices: **Warrior, Wizard, Thief, Cleric**. With no Class card in play, you have
**no Class** (no class abilities).
- Gained/lost exactly like Race (gain on play, lose on discard; discardable any
  time including mid-combat). Some Class abilities are powered by discards (any
  card, from hand or play).
- You may not hold more than one Class at once unless you play **Super
  Munchkin**.
- You may not have two copies of the same Class card in play.

Base-set Class abilities (actual card text):
| Class | Abilities |
| --- | --- |
| **Warrior** | **Berserking:** You may discard up to 3 cards in combat; each gives +1. **You win ties in combat.** |
| **Wizard** | **Flight Spell:** Discard up to 3 cards while Running Away; each gives +1 to flee. **Charm Spell:** Discard your whole hand to charm a single Monster instead of fighting it — discard the Monster and take its Treasure, but gain no level. If there are other monsters in the combat, fight them normally. |
| **Thief** | **Backstabbing:** Discard a card to backstab another player (−2 to their combat strength). Only once per victim per combat (but you may backstab each of two players fighting together). **Theft:** Discard a card to try to steal a small Item carried by another player; roll a die, 4+ succeeds, otherwise you lose 1 Level. |
| **Cleric** | **Resurrection:** When it is time to draw a card face-up, you may instead take the top card from the appropriate discard pile, then discard one card from your hand. **Turning:** Discard up to 3 cards in combat against an Undead creature; each gives +3. |

### 3.4 Sex
Each character has a sex, starting the same as the player's. It matters only for
cards that care about sex (e.g. monster bonuses against a sex, sex-restricted
Items). Race/Class card art does *not* change your sex. Changing sex (e.g. via
the *Change Sex* Curse) carries a standard **−5 combat penalty** while in effect.

---

## 4. Turn Structure

At the **start of your turn** (before Phase 1), you may freely: play cards,
switch Items between "in use" and "carried," trade Items with other players, and
sell Items for levels. When ready, proceed through the phases.

### Phase 1 — Kick Open the Door
Draw one card from the **Door deck** face up.
- **If it's a Monster:** you must fight it immediately (it attacks you). See
  §5. Resolve the combat completely before continuing.
- **If it's a Curse:** it applies to you immediately (if it can) and is
  discarded (continuing Curses stay in play). See §10.
- **Any other card:** put it in your hand, or play it immediately if legal.

### Phase 2 — Look for Trouble
*Only if you did NOT draw a Monster in Phase 1.* You may optionally play one
Monster from your hand and fight it, exactly as if you'd kicked it open.

### Phase 3 — Loot the Room
*Only if you did not fight a Monster in Phase 1 (kick) and did not Look for
Trouble.* Draw one card from the **Door deck face down** and put it in your hand.
- If you **fought a monster but ran away**, you do **not** loot the room.

### Phase 4 — Charity
If you have more than your hand limit (5 cards; 6 for a Dwarf) at end of turn:
- You must play enough cards to get down to the limit, **or** give the excess to
  the player(s) with the **lowest Level**.
- If several players tie for lowest, divide the excess as evenly as possible
  (you choose who gets any larger leftover set).
- If **you** are the (tied-for-)lowest, just discard the excess.

Then it becomes the next player's turn (to the left).

> Phases 1–3 are partly mutually exclusive: drawing a monster on the kick sends
> you straight to combat; Look for Trouble and Loot the Room are alternatives
> for a no-monster kick.

---

## 5. Combat

Combat begins the instant a Monster card is revealed/played into a fight, and
must be fully resolved before any other action. While the fight is on, the
fighter (and anyone) is subject to the timing restrictions below.

### 5.1 Combat strength
- **Your combat strength** = your **Level** + all modifiers (positive and
  negative) from Items in use and other cards/abilities currently applying.
- **Monster combat strength** = its printed Level + all monster enhancers and
  other modifiers applied to it.
- Combat strength **can be negative** (Curses, backstabs, penalties), even
  though Level cannot go below 1.

### 5.2 Outcome
- **You win** if **your combat strength is strictly greater** than the
  monster's. You **kill** the monster: go up the number of levels shown on its
  card (`bottom_left`, usually 1, sometimes 2), and draw the number of Treasures
  shown (`bottom_right`).
- **You lose** (and must Run Away — §5.7) if the monster's combat strength is
  **equal to or greater than** yours.
- **Ties:** the **monster wins ties**, *unless at least one munchkin in the
  fight is a Warrior* (Warriors win ties). A Warrior helping you also lets you
  win ties.

### 5.3 Defeating without killing
Some cards/abilities (e.g. Wizard's Charm, *Magic Lamp*, *Pollymorph Potion*,
*Out to Lunch*) let you get rid of a monster **without killing** it. This still
counts as "winning" the encounter, but you **do not gain a level** for that
monster. Whether you get its Treasure depends on the card (see each card).

### 5.4 What you may / may not do during combat
- You **may** play one-shot Items (those marked "Usable once only"), either from
  your hand or already in play. Discard them after combat (win or lose).
- You **may** play Door cards that affect combat (Monster Enhancers, Wandering
  Monster, Curses, etc.).
- You **may** discard cards to power Class/Race abilities (Berserking, Turning,
  Flight Spell, Charm, backstab/theft per their rules).
- You **may not** sell, trade, steal, equip, un-equip, or play non-one-shot
  Items from your hand; you may not change Items between "in use" and "carried."
- Theft (Thief) cannot be used while the thief or the target is in combat.
- Once a monster is exposed, you fight with your equipment **as it stands** plus
  any one-shots you choose to play.

### 5.5 Resolving a kill
After you reach a winning state, discard the monster card plus any enhancers and
one-shots played on it, and draw Treasure. **But:** other players get a
"reasonable time" (jokingly "2.6 seconds") to respond — to play a Curse, a
Wandering Monster, an enhancer, etc., *just as you think you've won.* Only after
that window passes is the kill final (you then really get the levels and
Treasure). This "reasonable time to respond" applies to **any** defeat, killing
or not.

> **Rule:** You cannot collect *any* rewards (Treasure, levels) in the middle of
> a combat. Finish the entire fight first.

### 5.6 Fighting multiple monsters
Some cards (notably **Wandering Monster**, and **Mate**) add monsters to a
fight. In addition, the base set's **Undead** monsters have a built-in way to
join: you may play any **Undead** monster from your hand into a combat that
already contains an Undead monster to "help" it — **without** spending a
Wandering Monster card. (The base set has several Undead monsters: *Undead
Horse*, *King Tut*, *Ghoulfiends*, *Wight Brothers*, *Squidzilla*.) The result
is a normal multi-monster fight resolved as below.
- You must defeat the **combined combat strength** of all monsters at once.
- Monsters fight **side by side**: any one monster's special immunity or
  weakness (e.g. "fire weapons don't work," "won't fight X") applies to the
  whole group as a condition of the encounter. **However**, flat combat
  bonuses/penalties are *not* shared — a −2 on one monster is a −2 for the group,
  not −2 per monster.
- Special abilities that change *how* you fight (e.g. "fight with Level only")
  apply to the **entire** fight.
- You **may** eliminate one monster (with a card/ability that removes a single
  monster) and fight the rest normally — but you **cannot** kill/fight one and
  *run from* the others. It's all together.
- If you eliminate one monster but then run from the rest, you get **no
  Treasure** at all.
- You cannot grab one monster's treasure mid-fight (e.g. charm one, take loot,
  then fight its mate). All rewards come only after the whole combat is won.
- A monster enhancer played into a multi-monster fight: the player choosing
  which monster it applies to is the one who plays it. (Exception: **Mate** — see
  §8.2 — anything enhancing a monster also enhances its Mate.)

### 5.7 Asking for help
If you can't win alone, you may ask another player to help. Ask players one at a
time (any order) until someone agrees or all refuse.
- **Only one** player may help you. Their combat strength is **added** to yours.
  (Anyone else may still play cards to affect the combat, but only one *helper*.)
- You may **bribe** the helper with Items you carry and/or some/all of the
  monster's Treasure (agree how Treasure is split — who picks first, etc.).
- The monster's special abilities/vulnerabilities apply to the **helper** too,
  and vice versa. Examples:
  - A **Warrior** helper lets the team win ties and may Berserk (once per
    combat).
  - If a monster gets +X against Elves and your **Elf** helper joins, that bonus
    applies (unless already counted, or you're an Elf too — bonuses don't stack
    for being an Elf twice).
- When the team wins, the monster is slain: **you** (the turn player) go up a
  level for each monster slain and **you** draw all the Treasure (even if the
  helper's ability did the killing); distribute per your bribe agreement.
- The **helper does not gain a level** — *unless the helper is an Elf*, in which
  case the Elf gains 1 level per monster slain.

### 5.8 Interfering with someone else's combat
Any player may interfere by:
- Playing a **one-shot Item** (help or hinder — you can "accidentally" hit a
  friend).
- Playing a **Monster Enhancer** (usually makes the monster stronger and worth
  more Treasure).
- Playing a **Wandering Monster** (with a monster from hand) to join the fight.
- **Backstabbing** the fighter (if you're a Thief).
- Playing a **Curse** on the fighter.

### 5.9 Running Away
If you cannot win (no help, or help still isn't enough), you must Run Away.
- You get **no levels and no Treasure**, and you do **not** get to Loot the Room.
- **Roll one die per monster.** You escape from a monster on a **5 or 6** ("5 or
  better"), modified by:
  - Race/Item modifiers (e.g. Elf +1, Halfling −1, *Boots of Running Really
    Fast*, Wizard Flight Spell discards).
  - Some monsters impose a penalty to the roll (they're fast).
- **If you escape** a monster: discard it; usually no bad effect — but read the
  card, some monsters hurt you even on escape.
- **If a monster catches you** (failed roll): suffer that monster's **Bad
  Stuff** (printed on the card) — ranging from losing an Item, to losing levels,
  to Death.
- **Multiple monsters:** roll separately for each, in any order you choose;
  suffer each catcher's Bad Stuff as it catches you. (If a catcher kills you, you
  are excused from the remaining monsters' Bad Stuff — you're dead.)
- **Two cooperating players who both lose:** both must flee, rolling separately;
  the monster(s) can catch both.
- A monster whose text says it "**will not pursue** anyone of Level X or below"
  — if you are at/below that Level and lose, you **escape automatically** (no
  roll). But you still get no Treasure.
- You may **not** choose to Run Away if you are already winning with what's in
  play. (You are never *required* to play one-shots/enhancers to win, though, so
  by declining to play them you can choose to lose.)
- You may not swap Items or otherwise re-equip before rolling to Run Away.

### 5.10 Death
You die when you suffer Bad Stuff (or another card) that says you die.
- **You lose all your Items and your entire hand.** You **keep** your Level,
  Race(s), Class(es), and any continuing Curses that were on you (your new
  character looks exactly like the old one — including persistent Curses like
  Sex Change, Big Feet, Chicken on Your Head, Half-Breed, Super Munchkin).
- **Looting the Body:** Lay out the dead player's hand beside their in-play
  cards. Starting with the **highest-Level** living player and descending, each
  other player takes **one card** (break Level ties with a die roll). If the
  corpse runs out of cards, the rest get nothing. After everyone has taken one,
  discard all remaining corpse cards.
- A dead character **cannot receive cards for any reason** (not even Charity) and
  **cannot level up.**
- A dead character's **new** body appears when the **next player's turn starts**.
  From that moment the new (card-less) character may help others in combat.
- Death lasts only until the next player's turn begins.
- **On your next turn:** start by drawing **4 cards from each deck face down**,
  play any legal Race/Class/Item cards (as at game start), then take your turn
  normally.

---

## 6. Treasure

When you defeat a monster (kill *or* card-eliminate, per the card), you draw the
number of Treasures shown on its card (`bottom_right`).
- Draw **face down** if you killed it **alone**.
- Draw **face up** (so everyone sees) if **someone helped** you.
- Monster Enhancers may add extra Treasures (e.g. Enraged +1, Humongous +2).
- Treasure cards can be played as soon as drawn: Items can be placed in front of
  you; "Go Up a Level" cards can be used instantly (and on any player at any
  time — see §10.3).
- All Treasure for a multi-monster fight is drawn only after the whole fight is
  won.

---

## 7. Card Timing Reference (when each card can be played)

| Card type | When it can be played |
| --- | --- |
| **Monster** | If drawn **face up** during Kick Open the Door, it immediately attacks the drawer. If acquired any other way, it goes to your hand and may be played during **Look for Trouble**, or onto another player's combat via **Wandering Monster**. Each Monster card is one monster, even if its name is plural. **Undead exception:** an **Undead** monster may be played from your hand directly into any combat that *already* has an Undead monster in it, to "help" that Undead — **no Wandering Monster card needed** (see §5.6). |
| **Monster Enhancer** | By any player during any combat. All enhancers on one monster add together. Negative enhancers are allowed. |
| **Item (to the table)** | As soon as you get it, or any time on your own turn except during combat (unless the card says otherwise). |
| **One-shot Item ("Usable once only")** | During any combat, from hand or table. Some are usable outside combat too (e.g. *Wishing Ring*). Discarded after use. |
| **Item Enhancer** | Onto an Item you already have in play (cannot be played by itself; cannot be moved to another Item later). |
| **Other Treasure / specials** (e.g. "Go Up a Level") | Any time, unless the card says otherwise. Follow the card, then discard (unless it has a persistent effect). |
| **Curse** | If drawn **face up** during Kick Open the Door, applies to the drawer. If drawn face down or acquired otherwise, may be played on **any player at any time** (including the instant someone thinks they've won a fight). |
| **Race / Class** | To the table as soon as acquired, or any time during your own turn. |
| **Half-Breed / Super Munchkin** | Same as Race/Class, but you must already have a Race (for Half-Breed) or a Class (for Super Munchkin). |
| **Hireling** | Any time, even in combat, as long as you have only one in play. A face-up Hireling may go to hand instead. May be discarded any time. |

---

## 8. Card Categories in Detail

### 8.1 Items
Each Item card has a **name**, a **power** (combat bonus and/or effect), a
**size**, and a **value in Gold Pieces** (or "No Value").

- An Item in **hand** does nothing until played; once played it is **"carried."**
- **Big items:** You may carry only **one Big item** at a time (unless a card,
  e.g. the Dwarf race or a Hireling, lets you carry more). Any item not marked
  Big is **Small**. You cannot simply discard a Big item to play another — you
  must sell it, trade it, lose it to a Curse/Bad Stuff, or discard it to power an
  ability.
  - If you lose the ability that let you have extra Big items (e.g. stop being a
    Dwarf, or your Hireling dies), you must immediately fix it: sell the excess
    (if it's your turn, not in combat, and you have ≥1,000 GP of Items to sell),
    else give them to the lowest-Level player(s) who can carry them, else
    discard.
- **Use restrictions & slots:** Anyone can *carry* any Item, but to gain its
  bonus you must meet its restrictions and have a free slot:
  - Class/Race restrictions (e.g. *Mace of Sharpness* counts only for a Cleric;
    *Bow with Ribbons* for an Elf). The bonus counts only while you currently
    meet the restriction.
  - Slot limits: at most **one Headgear, one Armor, one Footgear**, and **two
    "1 Hand" items (or one "2 Hands" item)** in use at once. (A card showing
    "−1 Hand" *grants* an extra Hand; you're fine as long as Hands in use total
    2 or fewer.)
  - Items you carry but can't currently use (extra slots, wrong Class/Race) are
    **"carried" but not "in use"** — turn them sideways. They give no bonus.
- You may **not** change Items between "in use" and "carried" during combat or
  while Running Away.
- You cannot discard Item cards "just because." Items leave play only by: a sale,
  a trade/gift, powering an ability, or being forced out by a Curse/Bad Stuff (or
  the forced-Big-item rule above).

#### Trading
- You may trade **Items only** (not other cards), and only Items **in play** (not
  from hand).
- Trade any time **except during combat**. Any Item received in a trade must go
  **into play**; you can't sell it until your own turn.
- You may also **give** Items away without a trade (e.g. to bribe).

#### Selling Items for levels (§9.4 cross-ref)
On your turn (not in combat), discard Items totaling **at least 1,000 Gold
Pieces** to immediately go up **one level**. 2,000 GP = two levels, etc. No
change is given for overpayment.
- You may sell Items from your hand as well as in play.
- "No Value" Items count as 0 GP and may be included in a sale (but a sale must
  reach the 1,000 GP minimum, and you can't sell Items totaling under 1,000 just
  to remove them).
- **You may not sell to reach Level 10.**

### 8.2 Monster Enhancers
Cards that raise or lower a single monster's combat strength (and usually adjust
its Treasure yield). Playable by any player during any combat; all enhancers on a
monster sum. Base-set enhancers:

| Enhancer | Effect | Treasure change |
| --- | --- | --- |
| **Baby** | −5 to monster Level (min 1) | Draw 1 fewer Treasure (min 1) |
| **Enraged** | +5 to monster Level | +1 Treasure |
| **Intelligent** | +5 to monster Level | +1 Treasure |
| **Humongous** | +10 to monster Level | +2 Treasures |
| **Ancient** | +10 to monster Level | +2 Treasures |
| **Mate** | A second identical monster appears (same Level and all same bonuses). Treated like a Wandering Monster — two separate monsters, defeated/fled individually. | Draw treasure and gain levels for each if both defeated; player is at −1 to Run Away if fleeing. |

**Mate** special: it duplicates the monster *and its Monster Enhancers* (not
other card types). Anything that enhances a monster also enhances its Mate. If
you remove the original monster from the fight *before* a Mate is played, the
Mate can't be played (no monster to join). If you remove one of the pair after
both exist, you must still deal with the other separately.

### 8.3 "Go Up a Level" and other specials
"Go Up a Level" cards grant one level instantly and may be played on **any player
at any time** (including to push a rival to a Level a monster will now chase).
The target must be *able* to legally gain that level — you **cannot** use one to
take a player to Level 10 (which requires a kill). Base-set Treasure "Go Up a
Level" cards include: *1,000 Gold Pieces*, *Boil an Anthill*, *Bribe GM With
Food*, *Invoke Obscure Rules*, *Kill the Hireling* (only if a Hireling is in
play, no matter whose; discard it), *Mutilate the Bodies* (only after a combat,
not necessarily yours), *Whine at the GM* (not usable if you're the highest-Level
player or tied for highest). *Divine Intervention* (a Door card) levels up **all
Clerics** the moment it's revealed, however drawn. *Steal a Level* takes one
level from a chosen player and gives it to you.

### 8.4 Combat-manipulation Treasures (examples)
- ***Magic Lamp*** — Your turn only. Makes a single monster vanish (even after a
  failed Run Away roll). If it was the only monster, take its Treasure but gain
  no level. In a multi-monster fight, removing one monster yields none of *that*
  monster's Treasure. Usable once only.
- ***Pollymorph Potion*** — During combat, turns one monster into a parrot that
  flies away leaving its Treasure. Usable once only.
- ***Out to Lunch*** (Door) — During any combat, the facing player discards all
  the monsters and immediately draws 2 Treasures.
- ***Instant Wall*** — Lets one or two willing characters escape any fight
  automatically. Play it after deciding to Run Away but before rolling. Usable
  once only.
- ***Illusion*** (Door) — During combat, discard one monster (and cards modifying
  it) and replace it with a monster from your hand.
- ***Help Me Out Here!*** (Door) — While you're in combat, take one Item from any
  player; at that moment it must make the difference between winning and losing.
  You may discard one of your own Items first if you wish. The Item taken must
  itself raise your combat strength enough to win (or immediately lead to that):
  you can't use it to grab a remove-the-monster card (Magic Lamp, Pollymorph
  Potion, etc.), an Item too weak to put you over, or anything that doesn't
  change your combat strength, and you can't use it at all if you're already
  winning.
- ***Mate***, ***Wandering Monster*** — add monsters (see §5.6, §8.2).
- ***Pretty Balloons*** — distraction during any combat; +5 to either side.
  Usable once only.
- ***Doppleganger*** — Summons your exact duplicate to fight beside you, doubling
  your effective Level and all bonuses. Usable **only if you are the only player
  in the combat** (no helper). Usable once only.
- ***Transferral Potion*** — During any combat, hand the fight to another player
  of your choice: they fight the monster(s), may ask for help, and get the
  Treasure and levels if they win; you then resume your turn (and may Loot the
  Room, win or lose). **It is still *your* turn**, so the player you handed the
  fight to may **not** use "your-turn-only" cards (e.g. *Magic Lamp*). Usable
  once only.
- ***Friendship Potion*** — Makes a monster befriend you and leave. Cannot be used
  **after** you've failed a Run Away roll — the combat ended when you failed to
  kill it. Usable once only.
- ***Loaded Die*** — Set a die to a value of your choice. You **physically turn
  the die** to the chosen face (you can't name an arbitrary number); any roll
  modifiers then apply on top, exactly as if you'd rolled that number.
- ***Wand of Dowsing*** — Go through a discard pile and take any one card. Treated
  as a **one-shot** for timing despite not saying "Usable once only": it may be
  played from your hand and during combat. Discarded after use.

### 8.5 Curses
See §10.

---

## 9. Hirelings (base set: 1 Door card)

- The base **Hireling** "follows you around and carries things." It lets you
  carry and use one **extra Big item**. It **will not fight for you.**
- You may have only **one Hireling** in play; play one any time (even in combat);
  discard one any time.
- A Hireling is **not an Item** unless it has a Gold-Piece price (the base
  Hireling has none, so it can't be traded or sold).
- **Sacrifice:** Instead of rolling to Run Away, you may discard one Hireling
  (and anything it carries) to **automatically escape all monsters** in the
  fight, even if a card says escape is impossible. If someone was helping you,
  *you* decide whether they also escape automatically or must roll.
- **Monster reactions:** A monster's bonus/penalty against a Race/Class/Sex
  applies to you based on the Hireling's traits *only* for monster reactions
  (e.g. a Dwarf Hireling triggers a monster's anti-Dwarf bonus unless you discard
  the Hireling). Otherwise ignore the Hireling's Race/Class/Sex; Bad Stuff does
  not affect Hirelings unless it names them.
- Items a Hireling carries count as yours and are affected by Curses/Traps/Bad
  Stuff as if you carried them. If the Hireling is sacrificed, its items are
  lost; if it's killed, you loot its items; if it's taken away (Trap/Curse/Bad
  Stuff/loyalty change), its items go with it.
- *Kill the Hireling* (Treasure) discards any in-play Hireling and levels you up.

---

## 10. Curses

- **Drawn face up on Kick Open the Door:** applies to the drawer immediately.
- **Otherwise:** may be played on **any player at any time** — including the
  instant a player believes they've killed a monster.
- Most Curses apply immediately and are discarded. Some give a delayed or
  continuing penalty: keep such a card in play until the Curse is removed or its
  penalty triggers.
  - A "your next combat" Curse played on you *while you're in combat* counts in
    **that** combat (the current unresolved combat is your "next" one).
  - Continuing Curses kept on the table **cannot** be discarded to power
    Class/Race abilities, and cannot otherwise be voluntarily discarded.
  - Continuing Curses **persist through Death** (your new character keeps them).
- If a Curse could apply to more than one of your Items, **you (the victim)
  decide** which is affected — unless the card specifies who/how (e.g. "player to
  the right," "random").
- If a Curse applies to something you don't have, **ignore it** and discard it
  (e.g. "Lose Your Armor" with no armor).
- The exact wording matters (e.g. "Lose the Armor You Are Wearing" = an
  in-use Armor; "Lose One Armor" = any one Armor card in play; "Lose Your Armor"
  = discard all Armor in play). Cheated Items still count as their type.
- You may play a Curse (or Monster) on yourself, or "help" a rival in a way that
  costs them, if it benefits you.

### 10.1 Base-set Curse cards
The base set contains 14 Curse cards, including named ones such as *Change Sex*,
*Change Race*, *Duck of Doom*, *Lose 1 Small Item*, and a *Truly Obnoxious
Curse!*; the rest are generic "Curse!" cards. Resolve each per its printed text.

### 10.2 Order of resolution
Cards resolve **as they are played**, in the order they happen (Munchkin has no
"stack"). You must fully resolve a Curse before doing anything else (e.g. you
can't sell a cursed Item to dodge a "lose an Item" Curse). Exceptions are cards
that explicitly cancel a previously played card (e.g. *Wishing Ring*).

### 10.3 "Go Up a Level" timing
"Go Up a Level" cards may be played on any player at any time; the target must be
legally able to gain that level (never to Level 10 by this means).

### 10.4 How in-play cards leave play (summary)
- **Race/Class (incl. Half-Breed/Super Munchkin):** discardable any time,
  including to power an ability (but not to power an ability *of the card being
  discarded*, unless that ability requires discarding it).
- **Continuing Curses in play:** cannot be discarded voluntarily.
- **Items:** the only tradeable cards; discardable only via sale, powering an
  ability, fulfilling Bad Stuff/Curse requirements, or the forced-Big-item rule.

---

## 11. Winning the Game

The **first player to reach Level 10 wins** — but you **must reach Level 10 by
killing a monster**, unless a card explicitly grants another way to win. No
amount of selling Items, "Go Up a Level" cards, or stealing levels can take you
*to* Level 10.

---

## 12. Conflicts: Cards vs. Rules

- In general, **cards override the base rules.** When a card disagrees with this
  document, follow the card.
- **Exception — these four core rules cannot be overridden** unless a card
  *explicitly* says it supersedes them:
  1. Nothing can reduce a player below **Level 1** (though combat strength can go
     below 1).
  2. You go up a level after combat **only if you kill a monster** (defeating
     without killing gives no level).
  3. You **cannot collect rewards** (Treasure, levels) in the middle of a
     combat — finish the fight first.
  4. You **must kill a monster to reach Level 10.**
- Monster Level and Treasure count, and character Level, can never go below 1.
- Anything not covered by rules or cards is not allowed unless all players agree.

---

## 13. Rulings & Edge Cases (for implementation)

These clarify ambiguous interactions; they are normative for running the game.

**Timing & resolution**
- Cards resolve immediately as played; there is no batched resolution phase.
- You are never locked into an announced decision unless a card/rule says so —
  but there are no take-backs once a card is played or a die is rolled.
- When told to draw two and keep one, an "immediate effect" on the kept card
  fires only when you choose to keep it; the discarded one never took effect.
- "Play immediately"/"as soon as drawn" cards (e.g. *Hoard!*) trigger the instant
  they enter your hand (including dealt in the opening hand, or routed to hand by
  another card — they still count as drawn face down).

**Combat**
- Monsters win ties unless a Warrior is in the fight (including a Warrior helper).
- With multiple monsters you may kill/charm one and fight the rest, but cannot
  fight some and flee others; they fight you together.
- Monster immunities/weaknesses are shared across all monsters in a fight; flat
  bonuses/penalties are **not** shared and do **not** multiply per monster.
- Monster combat bonuses **against different Races/Classes do stack** (e.g. +4
  vs Dwarves and +4 vs Elves = +8 against a Half-Breed Dwarf/Elf), unless a card
  says otherwise.
- Changing your Race/Class mid-combat (e.g. via Curse) changes which Race/Class
  bonuses you get. You can never benefit from two Races or two Classes in one
  combat without a card that allows it. **Exception:** bonuses already gained via
  discards (e.g. a Warrior who already Berserked) are kept even after losing the
  Class, but no further discards for that ability are allowed. You cannot discard
  a Warrior and play another to Berserk twice.
- Theft cannot occur while the thief **or** the target is in combat, but a thief
  may steal between two players when *neither* of them is in combat even if a
  *third* combat is happening elsewhere.
- A Thief who fails a theft loses 1 Level (never below 1) — a Level-1 Thief risks
  nothing. A Thief cannot backstab himself ("another player").
- Once you play a one-shot Item into a fight it is no longer yours — it can't be
  stolen or Cursed away (theft doesn't work in combat anyway). The one-shot stays
  in play until combat ends, then is discarded; the turn player discards all
  played cards (and sets discard order if it matters).
- During a Run Away, you must honor the die roll before doing anything else (you
  may Curse others before rolling, and again after escape/Bad Stuff is resolved,
  but not between the roll and its consequence).
- A "won't pursue Level X or below" monster: a low-Level player may choose to
  fight it, but if they can't win they **must** Run Away (auto-escape, no roll),
  and get no Treasure. They cannot just "ignore" a monster to claim its loot.
- The Wizard's **Charm** can be used only by the munchkin whose combat it is (the
  door-kicker or a helper), not to interfere with others' fights; with multiple
  monsters, charming one yields its Treasure only after the *whole* combat is won
  (and never lets you grab-and-flee).
- The Wizard's **Flight Spell** discards may be made after the Run Away roll.
- Cleric **Turning** vs multiple Undead is capped at 3 cards **per combat**, not
  per monster.
- An ability/card that "automatically defeats" a monster type still gives others
  a reasonable time to respond (they can Wandering-Monster in a new monster, but
  can't use the defeated monster's "bring a friend" rules).
- "Reasonable time to respond" applies to *any* defeat (kill or not). The "2.6
  seconds" is a joke; players must at least be able to read the relevant card,
  but cannot stall indefinitely or dig through cards hoping to find help.

**Items, cards, hand**
- "In play" = on the table (and represents what it says). Cards in hand are just
  cards. Effects targeting "Items" or "cards in play" never reach your hand
  unless they say "your hand."
- A forced discard that doesn't specify a source may come from hand **or** play
  (your choice).
- An **Item** is any card with a Gold-Piece value or "No Value." Treasures
  without a value are not Items and can't be sold.
- Theft/forced-give is *giving* only when voluntary on the giver's part — the
  giver chooses which card; a Thief's steal is not "giving."
- Potions: anything depicted as a liquid in a container counts as a Potion even
  if it doesn't say "Potion" (e.g. *Yuppie Water*). Other one-shot subtypes
  (Grenade, Ichor) require their exact word. Ichors are not Potions and vice
  versa.
- A "−1 Hand" item grants an extra Hand. You're legal while Hands in use sum to
  ≤ 2.
- Go by the card **text**, not the art (e.g. Hand count).
- You can change Items between "in use"/"carried" any time you're not in combat
  or otherwise engaged (not before a Run Away roll, not mid-Curse).

**Cheat!**
- *Cheat!* lets you carry and use **one** Item that would otherwise be illegal
  for you (wrong Class/Race, extra slot, extra Big item, extra Hands). Put the
  Cheat! card with that Item; discard it when you lose the Item. You must already
  own the Item — Cheat! can't take an Item from a player or the discard pile, and
  can't be moved to another Item later. Cheating an Item doesn't change its
  properties (a cheated small Item is still small; a cheated Armor is still
  Armor) and won't remove a Curse's negative effect on it.

**Races/Classes (multi-Race/Class cards)**
- Half-Breed: two Races (all advantages and disadvantages of each), **or** one
  Race with all advantages and none of its disadvantages (e.g. anti-Elf monsters
  get no bonus vs a half-Elf). Lose Half-Breed if you lose all your Race cards.
- You cannot stack two Half-Breeds (or two Super Munchkins) for 3+ Races/Classes,
  nor use them to hold the same Race/Class twice for double benefit. Only one copy
  of any given Race/Class in play.
- With Half-Breed/Super Munchkin you may swap one of your Races/Classes for
  another without losing the multi-card. Switching a Class is one atomic action
  (discard old, play new) so Items keyed to "if you lose your Class" are not lost
  if you replace immediately. You may even replace a Class with the same Class
  (e.g. to dodge Charity).
- Race/Class cards cannot be kept in play sideways "for later" — only Items can
  be in play but unused.

**Death**
- You die only from Bad Stuff (or a card) that says you die. While dead you can't
  receive cards or level up. You stay dead only until the next player's turn
  starts, when your card-less new character appears (keeping Level, Race, Class,
  continuing Curses) and may help in combat.
- Your character always *tries* to Run Away; you can't choose to die (hope for a
  bad roll instead).

**Misc**
- A character's starting sex matches the player's; sex matters only for cards
  that reference it; Race/Class art doesn't change it; a sex change carries −5
  while in effect.
- If two cards both let you re-roll the same situation, try the best first; on
  failure try the next, as long as you have abilities left.

---

## 14. Quick Combat Algorithm (pseudocode)

```
on monster revealed (kick / look-for-trouble / wandering / mate):
    combat = { monsters: [...], fighter, helper: none }
    loop:
        playerStrength = fighter.level + fighter.modifiers
                         + (helper ? helper.level + helper.modifiers : 0)
        monsterStrength = sum(m.level + m.enhancers for m in combat.monsters)
        apply monster special rules (e.g. "level only", race/class bonuses, "won't pursue")
        if playerStrength > monsterStrength
           OR (playerStrength == monsterStrength AND warriorInFight):
            tentative WIN
        else:
            tentative LOSE
        allow all players a reasonable response window (curses, enhancers,
            wandering monsters, one-shots, help offers, backstabs, charm, etc.)
        if state changed: continue loop
        else: break
    if WIN:
        for each monster killed: fighter.level += monster.levels   # not Treasure-eliminated ones
        draw Treasures (face down if solo, face up if helped) after ALL monsters resolved
        Elf helper gains 1 level per monster slain
        check win: fighter.level == 10  (must be via a kill)
    else:  # LOSE -> Run Away
        for each monster (player-chosen order):
            if monster "won't pursue" fighter: auto-escape
            else: roll d6 + run-away modifiers; escape on >= 5 (after penalties)
            if not escaped: apply monster Bad Stuff (may cause Death)
        no Treasure, no level, no Loot the Room
```
```

> Note: "defeat without killing" (Charm, Magic Lamp, Pollymorph, Out to Lunch)
> counts as a win for ending the encounter but grants no level for that monster.
