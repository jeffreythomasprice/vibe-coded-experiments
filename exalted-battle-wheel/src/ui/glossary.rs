//! Teaching content for the tooltip layer (`tip.rs`). Kept separate from `crate::battle` so the
//! domain types stay pure mechanics and this file is the single place to audit for rules
//! accuracy. Citations are printed book page numbers (see RULES.md's citation convention);
//! `document-search text --pages <printed + 2> <printed + 2> <pdf>` reproduces the source text.

use exalted_battle_wheel::battle::{ActionKind, SequenceKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Book {
    Core,
}

impl Book {
    fn title(self) -> &'static str {
        match self {
            Book::Core => "Exalted 2E",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pages {
    One(u16),
    Range(u16, u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Citation {
    pub book: Book,
    pub pages: Pages,
}

impl Citation {
    fn page(book: Book, page: u16) -> Citation {
        Citation { book, pages: Pages::One(page) }
    }

    fn range(book: Book, first: u16, last: u16) -> Citation {
        Citation { book, pages: Pages::Range(first, last) }
    }

    pub fn label(&self) -> String {
        match self.pages {
            Pages::One(page) => format!("{}, p. {page}", self.book.title()),
            Pages::Range(first, last) => format!("{}, pp. {first}\u{2013}{last}", self.book.title()),
        }
    }
}

/// Where an entry's authority comes from. App affordances (Undo, the wheel itself) genuinely
/// have no book text, so this makes that explicit instead of faking a citation. `quote` is
/// `None` where RULES.md only paraphrases or tabulates the source rather than quoting it
/// verbatim — the page citation is still exact, only the quote is omitted.
#[derive(Debug, Clone, Copy)]
pub enum Source {
    Book { quote: Option<&'static str>, cite: Citation },
    AppConvention,
}

#[derive(Debug, Clone, Copy)]
pub struct Entry {
    pub term: &'static str,
    pub what: &'static str,
    pub interacts: &'static str,
    pub source: Source,
}

fn book(page: u16, quote: &'static str) -> Source {
    Source::Book { quote: Some(quote), cite: Citation::page(Book::Core, page) }
}

fn book_unquoted(page: u16) -> Source {
    Source::Book { quote: None, cite: Citation::page(Book::Core, page) }
}

fn book_range_unquoted(first: u16, last: u16) -> Source {
    Source::Book { quote: None, cite: Citation::range(Book::Core, first, last) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Topic {
    // Header
    AppOverview,
    Undo,
    Redo,
    EventLog,
    CurrentTick,
    AdvanceTick,
    TeachingMode,
    Theme,
    ReactionCount,

    // Roster
    Roster,
    CombatantName,
    Side,
    JoinBattleSuccesses,
    Botch,
    AddCombatant,
    RemoveCombatant,
    StartBattle,
    FirstAction,
    NextActionTick,

    // Wheel
    TickWheel,
    TickSlot,
    NowMarker,
    BeyondHorizon,
    Markers,
    MarkerDuration,

    // Queue
    Queue,
    ReviseCombatant,
    PendingMarker,
    CancelSequenceEarly,

    // Hover card / combatant state
    DvPenalty,
    DvRefresh,
    StateNormal,
    StateGuarding,
    StateAiming,
    StateInactive,
    StateInSequence,

    // Action panel
    UpNow,
    ShapingSection,
    ActionSelect,
    ActionName,
    Speed,
    Reflexive,
    Flurryable,
    SpeedOverride,
    DvOverride,
    Declare,
    DeclareSequence,
    ShapeTerrestrial,
    ShapeCelestial,
    ShapeSolar,
    SequenceStep,
    CastSpeedOverride,
    AdvanceSequence,
    RejoinSuccesses,
    InterruptSequence,
    InterruptDistracted,
    SaveAction,
    ManageSavedActions,
    SavedActions,
    SavedSequenceStep,
    ActionEffects,

    // One per ActionKind, via action_topic()
    ActionAim,
    ActionAttack,
    ActionDash,
    ActionGuard,
    ActionInactive,
    ActionMiscellaneous,
    ActionMove,
    ActionFlurry,
    ActionActivateCharm,
    ActionClinch,
    ActionJoinBattleInProgress,
    ActionCustom,
}

/// Exhaustive over `ActionKind` so a new action cannot compile without a matching glossary entry.
pub fn action_topic(kind: ActionKind) -> Topic {
    match kind {
        ActionKind::Aim => Topic::ActionAim,
        ActionKind::Attack => Topic::ActionAttack,
        ActionKind::Dash => Topic::ActionDash,
        ActionKind::Guard => Topic::ActionGuard,
        ActionKind::Inactive => Topic::ActionInactive,
        ActionKind::Miscellaneous => Topic::ActionMiscellaneous,
        ActionKind::Move => Topic::ActionMove,
        ActionKind::Flurry => Topic::ActionFlurry,
        ActionKind::ActivateCharm => Topic::ActionActivateCharm,
        ActionKind::Clinch => Topic::ActionClinch,
        ActionKind::JoinBattleInProgress => Topic::ActionJoinBattleInProgress,
        ActionKind::Custom => Topic::ActionCustom,
    }
}

/// Exhaustive over `SequenceKind` so a new sorcery Circle cannot compile without a matching entry.
pub fn sequence_topic(kind: SequenceKind) -> Topic {
    match kind {
        SequenceKind::ShapeTerrestrial => Topic::ShapeTerrestrial,
        SequenceKind::ShapeCelestial => Topic::ShapeCelestial,
        SequenceKind::ShapeSolar => Topic::ShapeSolar,
    }
}

impl Topic {
    pub fn entry(self) -> Entry {
        match self {
            Topic::AppOverview => Entry {
                term: "The Battle Wheel",
                what: "A tick tracker for Exalted 2nd Edition combat.",
                interacts: "Combat time advances in ticks, roughly one second apiece. Rather than the book's paper, dice, or counter-pile methods, the wheel shows every combatant's next action tick at a glance and rotates as the current tick advances.",
                source: book(141, "Combat time passes in abstract increments called ticks, each of which is approximately one second long by default, but may vary slightly depending on what happens during the tick."),
            },
            Topic::Undo => Entry {
                term: "Undo",
                what: "Steps the battle log back one event.",
                interacts: "The battle is event-sourced: every declared action, tick advance, and roster change is a logged event, and the current state is always replayed from the start. Undo simply moves the replay cursor back, so redo remains possible until a new event is pushed.",
                source: Source::AppConvention,
            },
            Topic::Redo => Entry {
                term: "Redo",
                what: "Steps the battle log forward one event, reversing the last Undo.",
                interacts: "Only available immediately after an Undo; declaring any new event clears the redo history.",
                source: Source::AppConvention,
            },
            Topic::EventLog => Entry {
                term: "Event Log",
                what: "Lists every logged battle event and jumps the battle to any point in the log.",
                interacts: "The battle is event-sourced, so the state you see is always a replay of the log from the start; jumping just moves the replay cursor. Events after the current position stay listed but dimmed and can be jumped back into, until you push a new event, which discards them.",
                source: Source::AppConvention,
            },
            Topic::CurrentTick => Entry {
                term: "Current tick",
                what: "The tick the battle is on right now.",
                interacts: "Combat always advances from tick 0 forward, one tick at a time. A combatant becomes eligible to act the moment the current tick reaches her next action tick.",
                source: book(141, "Combat always advances from tick 0 forward one tick at a time until the end of battle."),
            },
            Topic::AdvanceTick => Entry {
                term: "Advance Tick",
                what: "Moves the current tick forward by one.",
                interacts: "All actions declared on a tick are resolved as of the start of that tick, so two combatants can act — and even kill each other — simultaneously. The tick cannot advance while someone whose next action tick has arrived still hasn't declared an action: everyone up must act before time moves on.",
                source: book(141, "When multiple characters act on the same tick, their actions occur simultaneously. Nothing actually happens until every action is rolled and the tick is concluded, so actions disregard the effects of ‘previous’ rolls made in the same tick."),
            },
            Topic::TeachingMode => Entry {
                term: "Teaching mode",
                what: "Turns the explanatory tooltips on or off.",
                interacts: "Switch it off once the tick system is second nature and the tooltips are only slowing down play; switch it back on any time you want the reminders back.",
                source: Source::AppConvention,
            },
            Topic::Theme => Entry {
                term: "Theme",
                what: "Switches between light and dark color schemes, or follows the system setting.",
                interacts: "System matches your OS or browser's light/dark preference and updates live if that preference changes.",
                source: Source::AppConvention,
            },
            Topic::ReactionCount => Entry {
                term: "Reaction count",
                what: "The highest number of successes rolled by anyone who simultaneously joined the fight at its start.",
                interacts: "It is fixed once the battle starts and used ever after: every combatant's First Action is (reaction count − her Join Battle successes), and anyone joining a fight already in progress uses this same frozen number.",
                source: book(141, "The reaction count for the combat scene is a value equal to the highest number of successes rolled by anyone who simultaneously joins at the start of combat."),
            },

            Topic::Roster => Entry {
                term: "Combatants",
                what: "Everyone who will act in this battle.",
                interacts: "Add every participant here before starting the battle — their Join Battle result decides who acts first.",
                source: Source::AppConvention,
            },
            Topic::CombatantName => Entry {
                term: "Name",
                what: "How this combatant is labelled on the roster, wheel, and hover card.",
                interacts: "Purely for your own reference; it has no mechanical effect.",
                source: Source::AppConvention,
            },
            Topic::Side => Entry {
                term: "Side",
                what: "Which faction this combatant fights for.",
                interacts: "Used to colour tokens on the wheel so allies and enemies are easy to tell apart at a glance. Combatants coordinating an attack together are typically all on the same side.",
                source: book_unquoted(144),
            },
            Topic::JoinBattleSuccesses => Entry {
                term: "Join Battle successes",
                what: "Successes on this combatant's reflexive Wits + Awareness roll to enter combat.",
                interacts: "This is the roll that decides turn order for the whole fight: the highest result among everyone joining simultaneously becomes the reaction count, and every combatant's First Action tick is (reaction count − her own successes), clamped to 0–6.",
                source: book_unquoted(141),
            },
            Topic::Botch => Entry {
                term: "Botch",
                what: "This combatant botched her Join Battle roll.",
                interacts: "A botched Join Battle roll forces a First Action of tick 6 regardless of the reaction count — the worst possible result, overriding the usual formula entirely.",
                source: book(141, "Any character who botches a Join Battle roll automatically has a First Action of 6."),
            },
            Topic::AddCombatant => Entry {
                term: "Add",
                what: "Adds this combatant to the roster with the entered name, side, and Join Battle result.",
                interacts: "Works during Setup or after Start Battle. Once the battle is started, the reaction count is frozen, so a newly-added combatant's First Action is scheduled straight from that frozen count instead of the live preview shown during Setup. A roster member who's present but hasn't formally engaged yet can also declare \"Join Battle, in progress\" from the action panel when she chooses to act instead.",
                source: Source::AppConvention,
            },
            Topic::RemoveCombatant => Entry {
                term: "Remove",
                what: "Removes this combatant from the battle entirely.",
                interacts: "During Setup, removing the roster's fastest joiner can lower the reaction count and reschedule everyone else's previewed First Action; once the battle has started the reaction count is frozen, so removing a combatant never touches anyone else's next action tick.",
                source: Source::AppConvention,
            },
            Topic::StartBattle => Entry {
                term: "Start Battle",
                what: "Locks in the reaction count from everyone's Join Battle results and schedules each combatant's First Action.",
                interacts: "First Action = (reaction count − successes), clamped to a maximum of 6; a botch forces First Action 6. From this point the reaction count no longer changes, even as new combatants join later.",
                source: book(141, "The First Action of each character equals (reaction count − successes), to a maximum value of 6."),
            },
            Topic::FirstAction => Entry {
                term: "First Action",
                what: "The tick on which this combatant is currently projected to act first, if the battle started right now.",
                interacts: "This is a live preview: it's recomputed from the current reaction count and this combatant's Join Battle successes every time the roster changes, and only becomes permanent when Start Battle locks in the reaction count. The fastest character (or characters, on ties) gets First Action on tick 0 and acts immediately.",
                source: book_unquoted(141),
            },
            Topic::NextActionTick => Entry {
                term: "Next action tick",
                what: "The tick on which this combatant is next free to act.",
                interacts: "Set to (the tick she acted on) + (the Speed of that action). She is inactive between now and then except for reflexive actions like Move, which never change this number.",
                source: book(141, "Once a character takes her first action in combat, she must wait a number of ticks equal to the Speed rating of her action before she acts again."),
            },

            Topic::TickWheel => Entry {
                term: "The tick wheel",
                what: "A rotating view of the next 12 ticks, with the current tick always at the top.",
                interacts: "Each combatant's token sits on the slot matching her next action tick. As the current tick advances, the wheel rotates so “now” stays fixed and everyone's position updates relative to it.",
                source: book(141, "Combat time passes in abstract increments called ticks … Combat always advances from tick 0 forward one tick at a time until the end of battle."),
            },
            Topic::TickSlot => Entry {
                term: "Tick slot",
                what: "One absolute tick number, twelve of which are visible at a time.",
                interacts: "Any combatant whose next action tick matches this slot's number has her token placed here.",
                source: book_unquoted(141),
            },
            Topic::NowMarker => Entry {
                term: "Now",
                what: "Marks the current tick at the top of the wheel.",
                interacts: "Everyone in this slot is eligible to act; the tick cannot advance past them until they declare an action.",
                source: book_unquoted(141),
            },
            Topic::BeyondHorizon => Entry {
                term: "Beyond the horizon",
                what: "Combatants whose next action is more than 12 ticks away, too far out to place on the wheel.",
                interacts: "This shouldn't normally happen for long — the highest fixed Speed in the core action catalog is 6, and even a fully-penalized weapon (missing every trait minimum) is capped at Speed 6, so ordinary actions land within the 12-tick window shown.",
                source: Source::AppConvention,
            },
            Topic::Markers => Entry {
                term: "Markers",
                what: "A labelled span of ticks you place on the wheel by hand, for anything the app doesn't track on its own.",
                interacts: "The book has several effects that last from a fixed tick until some future tick rather than following a combatant's own DV-refresh cycle — a coordinated attack's window of opportunity lasts from the moment it succeeds until the tick the commander next acts, and a saved action's effects (see Save) drop markers automatically when they resolve. Use a marker for anything similar: a Stunned penalty, a hazard, a standing order.",
                source: book(144, "If the roll succeeds, the coordination opens a \u{201c}window of opportunity\u{201d} on the tick when the commander next acts."),
            },
            Topic::MarkerDuration => Entry {
                term: "Duration",
                what: "How many ticks from now the marker starts, and how many ticks it spans once it starts.",
                interacts: "A one-tick marker (the default) covers only its starting tick — the shape of a coordinated attack's window of opportunity. A longer span suits an effect the book anchors to a future tick instead of to whoever it affects, such as a Stunned penalty that lasts until the tick when the attacker next acts.",
                source: book(153, "Failure leaves the victim at -2 dice to all non-reflexive rolls until the tick when the attacker next acts."),
            },

            Topic::Queue => Entry {
                term: "Queue",
                what: "Everything currently in flight, sorted by tick: who's due, who's resolving an action or a sorcery sequence, and every marker, whether it's already started or not.",
                interacts: "Click any row to open a full editor for it. Every edit there is appended as a new event, so Undo/Redo covers it exactly like any other declared action.",
                source: Source::AppConvention,
            },
            Topic::ReviseCombatant => Entry {
                term: "Revise combatant",
                what: "A full-override escape hatch: retime this combatant's next action, adjust her DV, force a state change, or clear what she's committed to.",
                interacts: "Applying this appends a correction event rather than rewriting history, so Undo reverts exactly this edit and nothing else — retcon an action to resolve in fewer ticks, then undo it, and the original tick comes back.",
                source: Source::AppConvention,
            },
            Topic::PendingMarker => Entry {
                term: "Pending marker",
                what: "A marker whose span hasn't started yet.",
                interacts: "A marker with a delay is invisible on the wheel and in the hover card until its start tick arrives — the queue is the only place to see or edit it before then.",
                source: Source::AppConvention,
            },
            Topic::CancelSequenceEarly => Entry {
                term: "Cancelling a sequence here",
                what: "Forcing a shaping combatant into any state other than \u{201c}In sequence (keep)\u{201d} abandons her spell.",
                interacts: "The book models losing a spell — whether to a failed distraction check or a voluntary choice — as dissipating harmlessly, with an immediate Join Battle roll to re-enter combat. This editor doesn't roll that Join Battle for you: use Interrupt in the action panel for the modeled rejoin, or set her next action tick here by hand.",
                source: book(251, "If the roll fails, the spell dissipates harmlessly and has no effects."),
            },

            Topic::DvPenalty => Entry {
                term: "DV penalty",
                what: "How much this combatant's last action degrades her Dodge and Parry DV.",
                interacts: "Applies from the moment she acts and lasts until her DV refreshes — normally at the start of her next action, though aborting a Guard or an Aim keeps the old penalty in place instead of refreshing.",
                source: book(147, "This penalty disappears on the tick the character is next permitted to act."),
            },
            Topic::DvRefresh => Entry {
                term: "DV refresh",
                what: "The tick on which this combatant's DV penalty clears.",
                interacts: "Refresh happens at the very start of the tick she's next permitted to act, before any new action's penalty is applied — so a Speed 5 action taken on tick 3 leaves her penalized for ticks 3–7 and clear again at the top of tick 8. Aborting out of Guard or Aim is the exception: the follow-up action does not refresh DV, it only reschedules the next action.",
                source: book(141, "most actions also have a defense penalty, determining how much the action reduces the character's Defense Value … until her next action refreshes this trait."),
            },
            Topic::StateNormal => Entry {
                term: "Normal",
                what: "No standing action state — free to declare any action next.",
                interacts: "The default; nothing here suppresses her next DV refresh.",
                source: Source::AppConvention,
            },
            Topic::StateGuarding => Entry {
                term: "Guarding",
                what: "Holding a defensive stance, ready to abort into another action.",
                interacts: "This is 2E's way of waiting for a better moment — there is no separate “delay” action. Guard imposes no DV penalty, and on any tick while guarding she may abort into any action except Aim or another Guard. The new action does not refresh her DV; she still has to wait out its full Speed before acting again.",
                source: book(143, "This new action does not refresh DV but is a normal action in all other ways. Therefore, the character must wait for a number of ticks to pass according to the Speed of the new action to refresh DV and act again."),
            },
            Topic::StateAiming => Entry {
                term: "Aiming",
                what: "Studying a specific target, building toward a bonus on the attack.",
                interacts: "Completing the full Speed 3 grants +3 bonus dice on the next attack against that target; aborting early to attack instead grants +1 die per tick spent aiming. Either way the attack does not refresh DV. Re-entering aiming instead of attacking banks the bonus for later without dropping DV any further.",
                source: book(142, "the attack does not refresh DV, even though it counts as a normal action in all other respects."),
            },
            Topic::StateInactive => Entry {
                term: "Inactive",
                what: "Unconscious, paralyzed, or otherwise not choosing her own actions.",
                interacts: "Not voluntary — it interrupts whatever she was doing the instant it applies, and while inactive she cannot defend herself at all (DV 0). It ends as abruptly as it began: on the next available tick she acts normally again with fully refreshed DV.",
                source: book(143, "On the next available tick, the character may act normally with refreshed DV and a full range of options."),
            },
            Topic::StateInSequence => Entry {
                term: "In a sorcery sequence",
                what: "Partway through shaping a spell: one to three Speed 5 shaping actions followed by a Cast whose Speed is set by a Join Battle roll (0–6), all of which must complete unbroken.",
                interacts: "While shaping she cannot use Charms or Combos (including reflexive ones) or take voluntary reflexive actions such as speech, Move, or Dash. If the sequence is broken, the spell is lost and she must make an immediate Join Battle roll to re-enter combat.",
                source: book(251, "cannot use Charms or Combos, including reflexive Charms. He cannot take voluntary reflexive actions, such as speech, Move or Dash."),
            },

            Topic::UpNow => Entry {
                term: "Up now",
                what: "Everyone whose next action tick has arrived and who must declare an action before the tick can advance.",
                interacts: "There is no passing: doing nothing is itself an action (typically Guard), so everyone listed here needs a declared action before Advance Tick will proceed. A combatant who is Inactive is the one exception — she isn't choosing her actions at all, so she's left off this list and never blocks the tick from advancing.",
                source: book(141, "Doing nothing is itself an action, whether a character is waiting in a guard position or paralyzed."),
            },
            Topic::ShapingSection => Entry {
                term: "Shaping",
                what: "Combatants partway through a sorcery sequence, even on ticks where they aren't otherwise due to act.",
                interacts: "A shaping sorcerer can be interrupted at any time by a distraction, not only on her own tick — so she's listed here for the whole shaping sequence, separately from the Up now list.",
                source: book_range_unquoted(251, 252),
            },
            Topic::ActionSelect => Entry {
                term: "Action",
                what: "The action this combatant is about to declare.",
                interacts: "Every action carries a Speed (ticks until her next action) and a DV penalty (how much it degrades her Dodge and Parry DV until it refreshes) — shown below once selected.",
                source: book_unquoted(141),
            },
            Topic::ActionName => Entry {
                term: "Name",
                what: "What this declared action is called in the event log. Leave blank to use the action's own name.",
                interacts: "Naming an action doesn't change its mechanics — it's still whatever kind is selected above, with that kind's Speed and DV rules. Use it to record which Attack maneuver, Charm, or house-ruled action this actually is.",
                source: Source::AppConvention,
            },
            Topic::Speed => Entry {
                term: "Speed",
                what: "How many ticks pass before this combatant can act again.",
                interacts: "Sets her next action tick: current tick + Speed. Speed 0 (only Move, and a few Speed-0 special cases) resolves immediately and does not consume her place in the cycle at all.",
                source: book_unquoted(141),
            },
            Topic::Reflexive => Entry {
                term: "Reflexive",
                what: "Can be taken on any tick, whether or not this combatant is otherwise due to act.",
                interacts: "Reflexive actions never refresh DV and don't count as a “true action” for effects that last until the character's next action — Move is the only reflexive entry in the core catalog.",
                source: book(141, "Reflexive actions do not refresh a character's DV, nor do they count as true actions for the purposes of effects that last until a character's next action."),
            },
            Topic::Flurryable => Entry {
                term: "Flurryable",
                what: "Whether this action can be one part of a flurry — several actions declared together on a single tick.",
                interacts: "A flurry's Speed is the highest Speed among its actions, and each action in it still imposes its own DV penalty, cumulatively. Aim and Guard can never be part of a flurry.",
                source: book(143, "In the case of attacks, a weapon cannot be used to attack more times in a flurry than its rate."),
            },
            Topic::SpeedOverride => Entry {
                term: "Speed override",
                what: "Lets you enter a Speed other than this action's default — needed whenever the actual Speed isn't fixed.",
                interacts: "What it means depends on the action selected: for Attack, the weapon or maneuver's own Speed (a weapon missing any of its trait minimums adds one to its Speed per missing dot, up to a ceiling of 6); for Flurry, the highest Speed among the flurried actions; for Activate Charm, whatever Speed the Charm specifies; for Join Battle in progress, the roll result. Ignored for any action whose Speed is fixed.",
                source: book(373, "For each dot the character is missing from any minimum, subtract one from the Accuracy and Defense of the weapon, and add one to its Speed (to a maximum total of Speed rating 6)."),
            },
            Topic::DvOverride => Entry {
                term: "DV override",
                what: "Lets you enter a DV penalty other than this action's default.",
                interacts: "What it means depends on the action selected: for Miscellaneous, the player's choice of forfeiting all DV for full concentration or taking only -1 (and -2 dice on the task) by keeping one eye on the battle; for Activate Charm, whatever penalty the Charm specifies. Ignored for any action whose DV penalty is fixed.",
                source: book_unquoted(143),
            },
            Topic::Declare => Entry {
                term: "Declare",
                what: "Resolves this action on the current tick: schedules her next action tick and applies her DV penalty right now.",
                interacts: "Only available once her next action tick has arrived. Contrast a sorcery selection, which instead starts a multi-tick sequence — see Declare (sorcery).",
                source: Source::AppConvention,
            },
            Topic::DeclareSequence => Entry {
                term: "Declare (sorcery)",
                what: "Starts a multi-tick sorcery sequence instead of resolving on this tick.",
                interacts: "Each Shape action is Speed 5 at the Circle's DV penalty; the closing Cast Sorcery action is DV -0 and its Speed is whatever you roll for Join Battle, not fixed. The whole sequence must run unbroken or the spell is interrupted.",
                source: book(252, "CAST SORCERY (VARIES, DV -0) … Determine the Speed of this action by making a Join Battle roll."),
            },
            Topic::ShapeTerrestrial => Entry {
                term: "Shape Terrestrial Circle Sorcery",
                what: "Begins shaping a Terrestrial Circle spell: one Speed 5 action at DV -2.",
                interacts: "Must be followed, unbroken, by a Cast Sorcery action or the spell is interrupted. While shaping, no Charms, Combos, or voluntary reflexive actions are allowed.",
                source: book(252, "SHAPE TERRESTRIAL CIRCLE SORCERY (SPEED 5, DV -2)"),
            },
            Topic::ShapeCelestial => Entry {
                term: "Shape Celestial Circle Sorcery",
                what: "Begins shaping a Celestial Circle spell: two consecutive Speed 5 actions at DV -3.",
                interacts: "Both shaping actions must complete unbroken before Cast Sorcery, or the spell is interrupted.",
                source: book(252, "SHAPE CELESTIAL CIRCLE SORCERY (TWO ACTIONS—EACH SPEED 5, DV -3)"),
            },
            Topic::ShapeSolar => Entry {
                term: "Shape Solar Circle Sorcery",
                what: "Begins shaping a Solar Circle spell: three consecutive Speed 5 actions at DV -4.",
                interacts: "All three shaping actions must complete unbroken before Cast Sorcery, or the spell is interrupted.",
                source: book(252, "SHAPE SOLAR CIRCLE SORCERY (THREE ACTIONS—EACH SPEED 5, DV -4)"),
            },
            Topic::SequenceStep => Entry {
                term: "Sequence step",
                what: "Where this combatant is within her shape-then-cast sorcery sequence.",
                interacts: "The whole sequence must run unbroken: each Shape step is Speed 5, and the final Cast Sorcery step's Speed isn't fixed — it's determined by rolling Join Battle.",
                source: book_range_unquoted(251, 252),
            },
            Topic::CastSpeedOverride => Entry {
                term: "Sequence speed override",
                what: "Overrides the Speed of this combatant's next sorcery step. Only meaningful on the final Cast Sorcery step, where the Speed is rolled via Join Battle rather than fixed.",
                interacts: "Enter the result of the Cast step's Join Battle roll while the display still shows the last Shape step, then click Advance — that click both moves the sorcerer onto Cast Sorcery and consumes this value to schedule it. Once the display already reads \"Cast Sorcery,\" this field no longer does anything; on any earlier Shape step it's likewise ignored, since Shape's Speed is always a fixed 5.",
                source: book_unquoted(252),
            },
            Topic::AdvanceSequence => Entry {
                term: "Advance",
                what: "Moves this combatant to the next step of her sorcery sequence.",
                interacts: "Only available once her next action tick has arrived, same as declaring any other action.",
                source: Source::AppConvention,
            },
            Topic::RejoinSuccesses => Entry {
                term: "Rejoin successes",
                what: "Successes on the immediate Join Battle roll made after a sorcery sequence is interrupted and the spell is lost.",
                interacts: "This new Join Battle roll works exactly like joining a fight already in progress: it schedules a fresh next action tick from the frozen reaction count, same as any other combatant re-entering the fray.",
                source: book(252, "If the character loses the spell due to distraction, he refocuses on the world, and the player makes an immediate Join Battle roll."),
            },
            Topic::InterruptSequence => Entry {
                term: "Interrupt",
                what: "Voluntarily breaks this combatant out of her sorcery sequence before it completes.",
                interacts: "Use this when the player is choosing to abandon the spell rather than continue the sequence. The app rules this the same as a failed distraction check: losing the spell either way forces an immediate Join Battle roll to re-enter combat, using the successes entered above — though the book only spells out that consequence explicitly for the distraction case. If a distraction — not a choice — broke her concentration, use Distracted instead.",
                source: book(252, "If the character does not do so, consider the spell interrupted."),
            },
            Topic::InterruptDistracted => Entry {
                term: "Distracted",
                what: "Records that this combatant was distracted while shaping and failed the roll to keep her concentration, losing the spell.",
                interacts: "The book models a distraction as a reflexive Wits + Occult roll at difficulty 1 to keep concentration; only a failed roll belongs here — a success means the sequence continues uninterrupted and there's nothing to declare. Losing the spell this way still forces an immediate Join Battle roll to re-enter combat, using the successes entered above.",
                source: book(251, "If the character is distracted, then his player must make a reflexive (Wits + Occult) roll for the Exalt to keep his concentration. This roll is difficulty 1."),
            },

            Topic::SaveAction => Entry {
                term: "Save\u{2026}",
                what: "Saves the currently selected action or sorcery — with its name, Speed, DV, and any effects — to your library for reuse.",
                interacts: "Starts from whatever is currently selected above: a renamed catalog action keeps its entered name and overrides, a sorcery keeps its Shape/Cast steps. Nothing is declared by saving — use Declare for that, or pick the saved entry later from the Saved group in the list above.",
                source: Source::AppConvention,
            },
            Topic::ManageSavedActions => Entry {
                term: "Manage\u{2026}",
                what: "Opens the list of saved actions to edit or delete them.",
                interacts: "Deleting a saved action only removes it from the library — it doesn't affect anything already declared with it, since a declared action's Speed, DV, and effects were copied in at declare time.",
                source: Source::AppConvention,
            },
            Topic::SavedActions => Entry {
                term: "Saved action",
                what: "A named action or sorcery you've saved for reuse, kept in this browser's local storage.",
                interacts: "Saved the same way across tabs: saving or deleting one here updates the Saved group in every open tab immediately, the same way Teaching mode or Theme does.",
                source: Source::AppConvention,
            },
            Topic::SavedSequenceStep => Entry {
                term: "Step",
                what: "One action in a saved sorcery sequence: its label, Speed, and DV penalty.",
                interacts: "Leave Speed blank to mark a step's Speed as rolled via Join Battle rather than fixed — the shape Cast Sorcery uses (RULES.md §5.1). A saved sequence isn't limited to the book's three Circles: use this to record a Charm or house rule with its own multi-action timing.",
                source: book(252, "CAST SORCERY (VARIES, DV -0) … Determine the Speed of this action by making a Join Battle roll."),
            },
            Topic::ActionEffects => Entry {
                term: "Effects",
                what: "Labelled spans this action drops onto the wheel the moment it resolves (or, for a sorcery, the moment its Cast resolves).",
                interacts: "Each effect gets its own marker, delayed by the ticks you set and lasting the duration you set — the same tick-anchored-span shape as a coordinated attack's window of opportunity. Use this for anything a saved action should leave behind: a hazard, a standing bonus, a Charm's lingering condition.",
                source: book(144, "If the roll succeeds, the coordination opens a \u{201c}window of opportunity\u{201d} on the tick when the commander next acts."),
            },

            Topic::ActionAim => Entry {
                term: "Aim (3/-1)",
                what: "Study a declared target to line up a better attack.",
                interacts: "Completing the full Speed 3 grants +3 bonus dice on the next attack against that target; aborting early instead grants +1 die per tick already spent aiming. Either way the eventual attack does not refresh DV. Cannot be part of a flurry.",
                source: book_unquoted(142),
            },
            Topic::ActionAttack => Entry {
                term: "Attack (weapon Speed/-1)",
                what: "A strike with a weapon or unarmed maneuver.",
                interacts: "Speed is the Speed of the weapon or maneuver used, not a fixed number — enter it as a Speed override. Can be flurried up to the weapon's Rate.",
                source: book(143, "The Speed of an attack is the Speed of the weapon or attack maneuver used."),
            },
            Topic::ActionDash => Entry {
                term: "Dash (3/-2)",
                what: "A full sprint, covering much more ground than a Move.",
                interacts: "Cannot be parried at all without a stunt or magic, on top of the -2 DV. A combatant can either Move or Dash on a given tick, never both.",
                source: book_unquoted(143),
            },
            Topic::ActionGuard => Entry {
                term: "Guard (3/-0)",
                what: "Hold a defensive stance instead of a fixed action, ready to abort into something else.",
                interacts: "2E's substitute for a “delay” action. No DV penalty while guarding; aborting into any action except Aim or another Guard does not refresh DV — the new action's Speed still has to elapse before she acts again. Cannot be part of a flurry.",
                source: book_unquoted(143),
            },
            Topic::ActionInactive => Entry {
                term: "Inactive (5/Special)",
                what: "Unconscious, paralyzed, helpless, or otherwise not acting by choice.",
                interacts: "Not voluntarily chosen — it interrupts a pending action the instant the condition arises. While inactive, DV is 0. It ends abruptly: on the next available tick she acts normally with fully refreshed DV.",
                source: book(143, "Characters who are inactive cannot defend themselves; they start the action at DV 0."),
            },
            Topic::ActionMiscellaneous => Entry {
                term: "Miscellaneous action (5/Varies)",
                what: "Anything that doesn't fit the other named actions — Speed 5 is roughly five seconds of work.",
                interacts: "The DV penalty is the actor's choice: forfeit all DV for full concentration, or take only -1 (and -2 dice on the task) with one eye on the battle. Only the latter can be part of a flurry.",
                source: book_unquoted(143),
            },
            Topic::ActionMove => Entry {
                term: "Move (0/None)",
                what: "Ordinary movement at Dexterity yards per tick.",
                interacts: "Reflexive: it never refreshes DV, doesn't count as a true action, and is available even on ticks she couldn't otherwise act. A combatant can either Move or Dash on a given tick, never both.",
                source: book_unquoted(145),
            },
            Topic::ActionFlurry => Entry {
                term: "Flurry (Varies/Varies)",
                what: "Several actions declared together on a single tick.",
                interacts: "Speed is the highest Speed among the flurried actions; each action still imposes its own DV penalty, cumulatively. A weapon cannot attack more times in a flurry than its Rate, and Aim and Guard can never be flurried.",
                source: book(143, "In the case of attacks, a weapon cannot be used to attack more times in a flurry than its rate."),
            },
            Topic::ActionActivateCharm => Entry {
                term: "Activate Charm / Combo / Power (Varies)",
                what: "Uses a Charm, Charm Combo, or other power as an action.",
                interacts: "A Simple Charm constitutes the whole action for the tick and defaults to Speed 6 unless the Charm lists its own Speed. Reflexive, Supplemental, and Extra Action Charms have their own separate timing and exclusion rules instead.",
                source: book_unquoted(142),
            },
            Topic::ActionClinch => Entry {
                term: "Clinch (6/-1)",
                what: "A grapple attempt: Speed 6, Rate 1, no damage on the initial hit.",
                interacts: "On a hit the attacker controls the clinch and the victim's action shifts immediately to Inactive. Maintaining the clinch requires using every subsequent action to renew it; the controller cannot block or dodge without a stunt or magic while doing so. The -1 DV is the standard Attack penalty (p.143), not something specific to grappling — the maneuver's own rules (cited below) only fix its Speed, Accuracy, and Rate.",
                source: book(157, "The maneuver has Speed 6, Accuracy +0 and Rate 1. This attack can be dodged or parried normally, and it inflicts no damage if it hits."),
            },
            Topic::ActionJoinBattleInProgress => Entry {
                term: "Join Battle, in progress (Varies/-0)",
                what: "How a combatant joins a fight that has already started.",
                interacts: "Speed is (the scene's frozen reaction count − her Wits + Awareness successes), clamped to 0–6 — the same formula used for everyone's original First Action, reusing the reaction count set when the battle began. On Speed 0 she isn't held back to a future tick at all: she proceeds immediately to declare another action for that tick, as if Join Battle itself had been reflexive.",
                source: book(144, "the character proceeds immediately to declare another action for that tick as if Join Battle was a reflexive action"),
            },
            Topic::ActionCustom => Entry {
                term: "Custom",
                what: "An action outside the core catalog, with Speed and DV penalty entered by hand.",
                interacts: "Use this for house rules, Charms with bespoke timing, or anything else the catalog doesn't name directly.",
                source: Source::AppConvention,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: &[Topic] = &[
        Topic::AppOverview,
        Topic::Undo,
        Topic::Redo,
        Topic::EventLog,
        Topic::CurrentTick,
        Topic::AdvanceTick,
        Topic::TeachingMode,
        Topic::Theme,
        Topic::ReactionCount,
        Topic::Roster,
        Topic::CombatantName,
        Topic::Side,
        Topic::JoinBattleSuccesses,
        Topic::Botch,
        Topic::AddCombatant,
        Topic::RemoveCombatant,
        Topic::StartBattle,
        Topic::FirstAction,
        Topic::NextActionTick,
        Topic::TickWheel,
        Topic::TickSlot,
        Topic::NowMarker,
        Topic::BeyondHorizon,
        Topic::Markers,
        Topic::MarkerDuration,
        Topic::Queue,
        Topic::ReviseCombatant,
        Topic::PendingMarker,
        Topic::CancelSequenceEarly,
        Topic::DvPenalty,
        Topic::DvRefresh,
        Topic::StateNormal,
        Topic::StateGuarding,
        Topic::StateAiming,
        Topic::StateInactive,
        Topic::StateInSequence,
        Topic::UpNow,
        Topic::ShapingSection,
        Topic::ActionSelect,
        Topic::ActionName,
        Topic::Speed,
        Topic::Reflexive,
        Topic::Flurryable,
        Topic::SpeedOverride,
        Topic::DvOverride,
        Topic::Declare,
        Topic::DeclareSequence,
        Topic::ShapeTerrestrial,
        Topic::ShapeCelestial,
        Topic::ShapeSolar,
        Topic::SequenceStep,
        Topic::CastSpeedOverride,
        Topic::AdvanceSequence,
        Topic::RejoinSuccesses,
        Topic::InterruptSequence,
        Topic::InterruptDistracted,
        Topic::SaveAction,
        Topic::ManageSavedActions,
        Topic::SavedActions,
        Topic::SavedSequenceStep,
        Topic::ActionEffects,
        Topic::ActionAim,
        Topic::ActionAttack,
        Topic::ActionDash,
        Topic::ActionGuard,
        Topic::ActionInactive,
        Topic::ActionMiscellaneous,
        Topic::ActionMove,
        Topic::ActionFlurry,
        Topic::ActionActivateCharm,
        Topic::ActionClinch,
        Topic::ActionJoinBattleInProgress,
        Topic::ActionCustom,
    ];

    #[test]
    fn every_entry_has_nonempty_text() {
        for topic in ALL {
            let entry = topic.entry();
            assert!(!entry.term.is_empty(), "{topic:?} has an empty term");
            assert!(!entry.what.is_empty(), "{topic:?} has an empty `what`");
            assert!(!entry.interacts.is_empty(), "{topic:?} has an empty `interacts`");
        }
    }

    #[test]
    fn every_book_citation_has_a_plausible_page_and_nonempty_quote() {
        for topic in ALL {
            let Source::Book { quote, cite } = topic.entry().source else { continue };
            if let Some(quote) = quote {
                assert!(!quote.is_empty(), "{topic:?} has an empty quote");
            }
            let pages = match cite.pages {
                Pages::One(p) => vec![p],
                Pages::Range(a, b) => vec![a, b],
            };
            for page in pages {
                assert!((120..=380).contains(&page), "{topic:?} cites implausible page {page}");
            }
        }
    }

    #[test]
    fn every_action_kind_has_a_topic() {
        for kind in [
            ActionKind::Aim,
            ActionKind::Attack,
            ActionKind::Dash,
            ActionKind::Guard,
            ActionKind::Inactive,
            ActionKind::Miscellaneous,
            ActionKind::Move,
            ActionKind::Flurry,
            ActionKind::ActivateCharm,
            ActionKind::Clinch,
            ActionKind::JoinBattleInProgress,
            ActionKind::Custom,
        ] {
            // Panics via the exhaustive match in `action_topic` if a variant is ever unhandled.
            let _ = action_topic(kind).entry();
        }
    }

    #[test]
    fn every_sequence_kind_has_a_topic() {
        for kind in [SequenceKind::ShapeTerrestrial, SequenceKind::ShapeCelestial, SequenceKind::ShapeSolar] {
            // Panics via the exhaustive match in `sequence_topic` if a variant is ever unhandled.
            let _ = sequence_topic(kind).entry();
        }
    }
}
