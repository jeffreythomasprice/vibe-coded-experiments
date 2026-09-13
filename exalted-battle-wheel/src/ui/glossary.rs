//! Teaching content for the tooltip layer (`tip.rs`). Kept separate from `crate::battle` so the
//! domain types stay pure mechanics and this file is the single place to audit for rules
//! accuracy. Citations are printed book page numbers (see RULES.md's citation convention);
//! `document-search text --pages <printed + 2> <printed + 2> <pdf>` reproduces the source text.

use exalted_battle_wheel::battle::{ActionKind, BattleMode, SequenceKind};

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
    Reset,
    ReactionCount,
    Room,
    RoomAdminModel,
    RoomRoleHost,
    RoomRoleAdmin,
    RoomRoleSpectator,
    RoomAdmin,
    RoomRename,
    RoomCopyCode,
    RoomKick,
    RoomStunServers,

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
    DvPenaltyRing,
    DvPenaltyFloor,
    SectorCountdown,
    MarkerGutter,
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

    // Battle mode
    BattleModeSelect,
    LongTick,

    // One per ActionKind, via action_topic() — personal/mass-shared entries
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

    // Personal-only named miscellaneous actions (RULES.md §4.7, p. 144)
    ActionCoordinateAttacks,
    ActionReadyWeapons,
    ActionRiseFromProne,
    ActionJump,

    // Mass combat only (RULES.md §11.1, pp. 162-164)
    JoinWar,
    ActionChangeFormation,
    ActionDisengage,
    ActionTurn,
    ActionSplitUnit,
    ActionExpelSpecialCharacter,
    ActionMergeUnits,
    ActionSignalUnits,
    ActionRally,

    // Social combat only (RULES.md §11.2, pp. 169-172) — separate topics because the Speed/DV
    // differ from the physical-combat entries above, not just the fiction
    JoinDebate,
    SocialMonologue,
    SocialAttack,
    SocialDash,
    SocialInactive,
    SocialMiscellaneous,
    SocialFlurry,
    ActionJoinDebateInProgress,
    ActionReadMotivation,
}

/// The glossary entries for one `ActionKind` across all three modes. `None` means the kind is
/// simply not an action in that mode — a unit does not Clinch, a debater does not Rally — rather
/// than "entry missing"; `every_catalog_action_has_a_topic` checks that against the real catalog.
#[derive(Debug, Clone, Copy)]
struct ModeTopics {
    personal: Option<Topic>,
    mass: Option<Topic>,
    social: Option<Topic>,
}

impl ModeTopics {
    /// The same topic in all three modes: the action's Speed/DV don't differ between them.
    const fn shared(topic: Topic) -> Self {
        Self { personal: Some(topic), mass: Some(topic), social: Some(topic) }
    }

    /// Physical combat only. Mass combat reuses `PERSONAL_CATALOG` verbatim (RULES.md §11.1,
    /// p. 166: characters there "substitute long ticks for standard ticks"), so a personal-only
    /// topic still applies there; social combat doesn't have the kind at all.
    const fn physical(topic: Topic) -> Self {
        Self { personal: Some(topic), mass: Some(topic), social: None }
    }

    const fn mass_only(topic: Topic) -> Self {
        Self { personal: None, mass: Some(topic), social: None }
    }

    const fn social_only(topic: Topic) -> Self {
        Self { personal: None, mass: None, social: Some(topic) }
    }

    /// One topic for both physical modes, a different one for social — used where the Speed
    /// and/or DV genuinely differ there (RULES.md §11.2, p. 171), not just the fiction.
    const fn split(physical: Topic, social: Topic) -> Self {
        Self { personal: Some(physical), mass: Some(physical), social: Some(social) }
    }

    const fn get(self, mode: BattleMode) -> Option<Topic> {
        match mode {
            BattleMode::Personal => self.personal,
            BattleMode::Mass => self.mass,
            BattleMode::Social => self.social,
        }
    }
}

/// Exhaustive over `ActionKind` so a new action still cannot compile without a glossary decision
/// for every mode — the same guarantee the original single-mode mapper gave, extended across the
/// mode axis with no wildcard arm. See `every_catalog_action_has_a_topic` for the half of the
/// guarantee this can't give at compile time: whether a topic is actually wired into the mode's
/// real catalog.
const fn topics_for(kind: ActionKind) -> ModeTopics {
    match kind {
        ActionKind::Aim => ModeTopics::split(Topic::ActionAim, Topic::SocialMonologue),
        ActionKind::Attack => ModeTopics::split(Topic::ActionAttack, Topic::SocialAttack),
        ActionKind::Dash => ModeTopics::split(Topic::ActionDash, Topic::SocialDash),
        ActionKind::Guard => ModeTopics::shared(Topic::ActionGuard),
        ActionKind::Inactive => ModeTopics::split(Topic::ActionInactive, Topic::SocialInactive),
        ActionKind::Miscellaneous => ModeTopics::split(Topic::ActionMiscellaneous, Topic::SocialMiscellaneous),
        ActionKind::Move => ModeTopics::shared(Topic::ActionMove),
        ActionKind::Flurry => ModeTopics::split(Topic::ActionFlurry, Topic::SocialFlurry),
        ActionKind::ActivateCharm => ModeTopics::shared(Topic::ActionActivateCharm),
        ActionKind::JoinBattleInProgress => ModeTopics::split(Topic::ActionJoinBattleInProgress, Topic::ActionJoinDebateInProgress),
        ActionKind::Clinch => ModeTopics::physical(Topic::ActionClinch),
        ActionKind::Custom => ModeTopics::shared(Topic::ActionCustom),

        ActionKind::CoordinateAttacks => ModeTopics::physical(Topic::ActionCoordinateAttacks),
        ActionKind::ReadyWeapons => ModeTopics::physical(Topic::ActionReadyWeapons),
        ActionKind::RiseFromProne => ModeTopics::physical(Topic::ActionRiseFromProne),
        ActionKind::Jump => ModeTopics::physical(Topic::ActionJump),

        ActionKind::ChangeFormation => ModeTopics::mass_only(Topic::ActionChangeFormation),
        ActionKind::Disengage => ModeTopics::mass_only(Topic::ActionDisengage),
        ActionKind::Turn => ModeTopics::mass_only(Topic::ActionTurn),
        ActionKind::SplitUnit => ModeTopics::mass_only(Topic::ActionSplitUnit),
        ActionKind::ExpelSpecialCharacter => ModeTopics::mass_only(Topic::ActionExpelSpecialCharacter),
        ActionKind::MergeUnits => ModeTopics::mass_only(Topic::ActionMergeUnits),
        ActionKind::SignalUnits => ModeTopics::mass_only(Topic::ActionSignalUnits),
        ActionKind::Rally => ModeTopics::mass_only(Topic::ActionRally),

        ActionKind::ReadMotivation => ModeTopics::social_only(Topic::ActionReadMotivation),
    }
}

/// `None` means `kind` is not an action in `mode` at all (a unit does not Clinch, a debater does
/// not Rally) — reachable in the UI only via `Option::map`, never via `.unwrap()`.
pub fn action_topic(mode: BattleMode, kind: ActionKind) -> Option<Topic> {
    topics_for(kind).get(mode)
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
            Topic::Reset => Entry {
                term: "Reset",
                what: "Clears the battle and starts a fresh one.",
                interacts: "The battle is saved to this browser automatically, so it's still here after a refresh — Reset is how you deliberately start over. It discards every combatant, declared action, marker, and undo step, and Undo cannot bring it back. Saved actions, theme, and Teaching mode are untouched.",
                source: Source::AppConvention,
            },
            Topic::ReactionCount => Entry {
                term: "Reaction count",
                what: "The highest number of successes rolled by anyone who simultaneously joined the fight at its start.",
                interacts: "It is fixed once the battle starts and used ever after: every combatant's First Action is (reaction count − her Join Battle successes), and anyone joining a fight already in progress uses this same frozen number.",
                source: book(141, "The reaction count for the combat scene is a value equal to the highest number of successes rolled by anyone who simultaneously joins at the start of combat."),
            },
            Topic::Room => Entry {
                term: "Multiplayer",
                what: "Connects browsers directly, peer-to-peer, with no server in between, so everyone sees the same battle and agrees on every change.",
                interacts: "One side hosts and shares a connection code; the other pastes it back and joins, adopting the host's battle immediately. After that, every edit anyone makes is proposed to the whole room and only takes effect once everyone agrees \u{2014} a genuine disagreement disconnects everyone with an error rather than quietly drifting apart. There's no relay server standing by, so two networks that both sit behind strict NATs (some phone hotspots, some corporate networks) may simply fail to connect to each other at all.",
                source: Source::AppConvention,
            },
            Topic::RoomAdminModel => Entry {
                term: "Everyone who joins starts as an admin",
                what: "Decides whether someone joining can change the battle straight away, or only watch until an admin says otherwise.",
                interacts: "It only sets the starting point for each new joiner. Once they're in, any admin can promote or demote anyone from the player list \u{2014} except themselves and the host, who is always an admin. The host refuses a change proposed by anyone who isn't an admin, so this isn't only a label; what it can't stop is a modified client voting badly on everyone else's changes.",
                source: Source::AppConvention,
            },
            Topic::RoomRoleHost => Entry {
                term: "Host",
                what: "This is the host \u{2014} the browser the room runs through.",
                interacts: "Every change anyone makes is proposed to the host, which puts it to the whole room and reports back the result. That's why the host is always an admin and can't be demoted, and why only the host can hand out invites or kick someone. If the host leaves, the room ends for everyone; each side keeps its own copy of the battle.",
                source: Source::AppConvention,
            },
            Topic::RoomRoleAdmin => Entry {
                term: "Admin",
                what: "This player is an admin in this room, so they can change the battle.",
                interacts: "They can add and remove combatants, declare actions, advance the tick, and undo \u{2014} and they can make anyone else an admin or take it away, except themselves and the host. Every change they make still has to be agreed by the whole room before it takes effect.",
                source: Source::AppConvention,
            },
            Topic::RoomRoleSpectator => Entry {
                term: "Not an admin",
                what: "This player is not an admin in this room. They can watch every change as it happens, but can't make any.",
                interacts: "Their copy of the battle stays in step with everyone else's, and they still vote on every change like everyone else \u{2014} they just can't propose one, so the controls that would change the battle are greyed out for them. Any admin can promote them at any time, and it takes effect immediately.",
                source: Source::AppConvention,
            },
            Topic::RoomAdmin => Entry {
                term: "Admin",
                what: "Whether this player may change the battle.",
                interacts: "Any admin can grant or revoke it for anyone else, but never for themselves and never for the host \u{2014} the host is always an admin. It takes effect the moment the host broadcasts the new roster: the affected player's controls grey out or light up without them doing anything. The host refuses any change proposed by someone who isn't an admin.",
                source: Source::AppConvention,
            },
            Topic::RoomRename => Entry {
                term: "Your name",
                what: "The name you're listed under in this room's player list.",
                interacts: "Change it whenever you like \u{2014} it's sent to everyone else as soon as you leave the field or press Enter. It's only a label: it has nothing to do with any combatant in the battle, and changing it doesn't affect what you're allowed to do.",
                source: Source::AppConvention,
            },
            Topic::RoomCopyCode => Entry {
                term: "Copy",
                what: "Puts the connection code on your clipboard, ready to paste into a chat or a message.",
                interacts: "The code is long by design \u{2014} it carries everything the other browser needs to reach yours directly, with no server in between. Paste it whole; a truncated code is rejected rather than half-connecting. If your browser won't give the page clipboard access, select the text in the box and copy it by hand instead.",
                source: Source::AppConvention,
            },
            Topic::RoomKick => Entry {
                term: "Kick",
                what: "Disconnects a peer from the room immediately.",
                interacts: "Host-only, and only for other peers \u{2014} you can't kick yourself. The kicked peer keeps their own copy of the battle; only the connection ends. There's no ban list behind this: if the host hands out another invite, whoever was kicked can use it to rejoin like anyone else.",
                source: Source::AppConvention,
            },
            Topic::RoomStunServers => Entry {
                term: "STUN servers",
                what: "The public-address lookup services a connection uses to work out how to be reached from outside your own network.",
                interacts: "Read only at the moment a connection is made, so an edit here applies to the next invite you create or room you join \u{2014} never to a connection already underway. It's the same list the app seeds from a `#stun=` URL fragment at page load; editing it here does the same thing without needing a fresh tab. Nothing here is saved: reloading the page restores the built-in defaults.",
                source: Source::AppConvention,
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
                interacts: "Used to colour tokens on the wheel so allies and enemies are easy to tell apart at a glance. Combatants coordinating an attack together are typically all on the same side. Factions already in the battle are offered as you type; picking one — or matching its spelling apart from capitalization — keeps everyone on that faction the same colour.",
                source: Source::AppConvention,
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
                what: "Seven wedges counting down to \u{201c}now\u{201d} at the top, with six rings marking DV penalty from the rim (-0) inward.",
                interacts: "A token's angular position is when she next acts; its distance from the rim is how badly her last action degraded her DV. As the current tick advances, tokens sweep toward the top wedge and slide outward as their DV penalty refreshes.",
                source: book(141, "Combat time passes in abstract increments called ticks … Combat always advances from tick 0 forward one tick at a time until the end of battle."),
            },
            Topic::TickSlot => Entry {
                term: "Sector",
                what: "One wedge of the wheel: how many ticks from now, not an absolute tick number.",
                interacts: "The small number under the big one is the absolute tick this sector currently represents. Any combatant whose next action tick lands this many ticks from now has her token placed somewhere in this wedge.",
                source: book_unquoted(141),
            },
            Topic::NowMarker => Entry {
                term: "Now",
                what: "Marks the current tick at the top of the wheel \u{2014} the wedge tokens sweep into as ticks advance.",
                interacts: "Everyone in this wedge is eligible to act; the tick cannot advance past them until they declare an action.",
                source: book_unquoted(141),
            },
            Topic::BeyondHorizon => Entry {
                term: "Beyond the horizon",
                what: "Combatants whose next action is more than 6 ticks away, too far out to place on the wheel.",
                interacts: "This shouldn't normally happen for long. RULES.md \u{a7}12.3 is explicit that nothing in the core rules states a global \u{201c}no action exceeds Speed 6\u{201d} \u{2014} it's only that every capped mechanic (First Action, Join-in-progress, the Minimums penalty, a Simple Charm's default Speed) happens to cap there. A hand-typed Speed above 6, or a marker running longer than that, is what lands here.",
                source: Source::AppConvention,
            },
            Topic::DvPenaltyRing => Entry {
                term: "DV penalty ring",
                what: "How far a token sits from the rim: its current DV penalty, from -0 at the rim to -5-or-worse at the hub.",
                interacts: "Refreshes at the very start of the tick a combatant is next permitted to act, before any new action's penalty applies \u{2014} so a token visibly slides back out to the rim the moment her DV clears.",
                source: book(147, "This penalty disappears on the tick the character is next permitted to act."),
            },
            Topic::DvPenaltyFloor => Entry {
                term: "-5 or worse",
                what: "The wheel's innermost ring, for any DV penalty of -5 or steeper.",
                interacts: "The core rules never cap how negative accumulated DV penalties can get, so a ring for every possible value isn't practical \u{2014} the hover card and queue panel always show the exact number.",
                source: Source::AppConvention,
            },
            Topic::SectorCountdown => Entry {
                term: "Seven sectors",
                what: "The wheel shows the next 6 ticks plus \u{201c}now,\u{201d} not an ever-growing tick count.",
                interacts: "Seven, not more, because RULES.md \u{a7}12.3 treats Speed 6 as this app's conventional ceiling for an ordinary action \u{2014} not a rule the book states outright, but the pattern every capped mechanic follows. An action scheduled further out than that shows up in \u{201c}Beyond the horizon\u{201d} instead of on the wheel.",
                source: Source::AppConvention,
            },
            Topic::MarkerGutter => Entry {
                term: "Marker arc",
                what: "An arc drawn just outside the rim, spanning the ticks a marker covers.",
                interacts: "Markers are spans of time, not a single combatant's state, so they sit outside the ring geometry entirely rather than competing with tokens for a ring position.",
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
                interacts: "The book spells this out for a failed distraction check: the spell dissipates harmlessly and the player makes an immediate Join Battle roll to re-enter combat. This app treats a voluntary abandonment the same way, though the book doesn't state that explicitly for the voluntary case. This editor doesn't roll that Join Battle for you: use Interrupt in the action panel for the modeled rejoin, or set her next action tick here by hand.",
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
                interacts: "Every action carries a Speed (ticks until her next action) and a DV penalty (how much it degrades her Dodge and Parry DV until it refreshes) — shown below once selected. Clicking a row in the reference rail selects it here for the highlighted \u{201c}Up now\u{201d} combatant, without declaring it.",
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
                interacts: "This new Join Battle roll works exactly like joining a fight already in progress: it schedules a fresh next action tick from the frozen reaction count, same as any other combatant re-entering the fray. The book states this explicitly for a failed distraction check; the app applies the same rejoin roll when the sequence is broken voluntarily too.",
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
                interacts: "Leave Speed blank to mark a step's Speed as rolled via Join Battle rather than fixed — the same convention Cast Sorcery uses. A saved sequence isn't limited to the book's three Circles: use this to record a Charm or house rule with its own multi-action timing.",
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
                source: book_range_unquoted(143, 145),
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
                source: book_range_unquoted(141, 145),
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

            Topic::BattleModeSelect => Entry {
                term: "Battle mode",
                what: "Which of the three tick-driven combat systems this battle runs: personal, mass, or social combat.",
                interacts: "All three share the same Speed/DV/refresh machinery; mass and social combat only change the scale of a tick and which actions are on the menu. Fixed before Start Battle, exactly like Join Battle successes \u{2014} it cannot change once actions are already on the wheel.",
                source: book_range_unquoted(158, 169),
            },
            Topic::LongTick => Entry {
                term: "Long tick",
                what: "Mass and social combat's unit of time: roughly one minute, not one second.",
                interacts: "Everything about the tick loop \u{2014} Speed, DV penalties, refresh timing \u{2014} works exactly the same way, just at this coarser scale. A character \u{201c}substitutes long ticks for standard ticks\u{201d} and may still use any reflexive Charm at any point in one.",
                source: book_range_unquoted(158, 166),
            },

            Topic::ActionCoordinateAttacks => Entry {
                term: "Coordinate Attacks (5/varies)",
                what: "Organizes a group attack: on success it opens a \u{201c}window of opportunity\u{201d} for everyone coordinated.",
                interacts: "Rolled as Charisma + War, difficulty equal to half the number of participants (round down). The DV choice is the same one Miscellaneous Action offers: forfeit all DV for full concentration, or keep one eye on the battle for -1 DV and -2 dice on the roll.",
                source: book(144, "The difficulty is half the number of participants in the group, rounded down."),
            },
            Topic::ActionReadyWeapons => Entry {
                term: "Draw / Ready Weapons (5/-1)",
                what: "Draws or readies as many weapons as the character has hands.",
                interacts: "Ready is normally automatic and diceless, sized to as many weapons as the character has hands and weapons available — the book gives it this exact -1 DV entry directly. A natural weapon like a punch or kick never needs readying; only the most extreme conditions (numb, frostbitten hands) call for a Dexterity + combat Ability roll at difficulty 1 instead.",
                source: book(144, "A character may use a miscellaneous action to unsheathe, draw or otherwise ready as many weapons as she has hands and weapons available."),
            },
            Topic::ActionRiseFromProne => Entry {
                term: "Rise From Prone (5/-1)",
                what: "Stands back up from prone.",
                interacts: "Being prone otherwise imposes a flat -1 external penalty on all non-reflexive physical actions. Rising is normally automatic; under extreme conditions it becomes a Dexterity + Athletics roll at difficulty 1.",
                source: book_unquoted(144),
            },
            Topic::ActionJump => Entry {
                term: "Jump (5/-1)",
                what: "A significant leap, distinct from ordinary movement.",
                interacts: "Only one jump is allowed per flurry or per action; a character may still Move normally on the same tick. A short jump that doesn't clear an obstacle worth vaulting doesn't need declaring at all \u{2014} it's just part of a normal Move.",
                source: book_unquoted(144),
            },

            Topic::JoinWar => Entry {
                term: "Join War",
                what: "Mass combat's version of Join Battle: schedules a unit's or solo hero's First Action.",
                interacts: "The dice pool is (Wits + War) minus the unit's Magnitude; a solo unit or an independently-acting hero instead rolls plain Wits + Awareness, same as personal combat. Either way the app only needs the resulting successes \u{2014} scheduling is the identical (reaction count \u{2212} successes) formula Join Battle uses.",
                source: book_unquoted(163),
            },
            Topic::ActionChangeFormation => Entry {
                term: "Change Formation (5/-1)",
                what: "Shifts a unit into a different formation: unordered, skirmish, relaxed, or close.",
                interacts: "Formation sets how fast a unit moves per long tick, from solo/skirmish at full speed down to unordered at less than a third \u{2014} tracked here only as a note on the combatant, since this app doesn't model position or movement.",
                source: book_range_unquoted(163, 165),
            },
            Topic::ActionDisengage => Entry {
                term: "Disengage (0/-0)",
                what: "Reflexively withdraws a unit from combat.",
                interacts: "Speed 0 and reflexive, like Move in personal combat \u{2014} it never costs a place in the tick cycle and never refreshes DV.",
                source: book_unquoted(165),
            },
            Topic::ActionTurn => Entry {
                term: "Turn, over 90\u{b0} (3/-1)",
                what: "Reorients a unit by more than a quarter turn.",
                interacts: "A turn of 90\u{b0} or less doesn't require this action at all; only the larger reorientation costs a tick's worth of time.",
                source: book_unquoted(165),
            },
            Topic::ActionSplitUnit => Entry {
                term: "Split Unit (3/-1)",
                what: "Divides one unit into two smaller ones.",
                interacts: "Both resulting units act independently afterward, each with its own place in the tick cycle from that point on.",
                source: book_unquoted(165),
            },
            Topic::ActionExpelSpecialCharacter => Entry {
                term: "Expel a Special Character (0/-0)",
                what: "Reflexively ejects one special character from the unit so she can act on her own.",
                interacts: "Speed 0 and reflexive \u{2014} the character cannot resist. Freed this way, she may in turn challenge her former commander to a duel instead of simply leaving.",
                source: book_unquoted(165),
            },
            Topic::ActionMergeUnits => Entry {
                term: "Merge Units (3/-1)",
                what: "Combines two units into one.",
                interacts: "The merged unit takes on a single place in the tick cycle going forward.",
                source: book_unquoted(165),
            },
            Topic::ActionSignalUnits => Entry {
                term: "Signal Units (3/-0)",
                what: "Relays an order to other units without breaking formation.",
                interacts: "No DV penalty \u{2014} signaling doesn't compromise the unit's guard the way most Speed 3 actions do.",
                source: book_unquoted(165),
            },
            Topic::ActionRally => Entry {
                term: "Rally (4/-1)",
                what: "A commander steps out to address the troops, with one of three effects: promoting a relay, recovering Magnitude lost to a failed morale check, or restoring Endurance.",
                interacts: "Each effect has its own Charisma + War/Performance roll; this app doesn't model Valor, morale, or Endurance, so Rally is tracked here only as a scheduled action, Speed 4 at -1 DV.",
                source: book_unquoted(165),
            },

            Topic::JoinDebate => Entry {
                term: "Join Debate",
                what: "Social combat's version of Join Battle: schedules a debater's First Action.",
                interacts: "Rolled as plain Wits + Awareness, identical to personal combat's Join Battle \u{2014} only the scale of the resulting ticks (long ticks, roughly a minute each) differs.",
                source: book(169, "The Join Debate action replaces Join Battle, with the roll using (Wits + Awareness) being made as normal. Time progresses forward in long ticks lasting one minute each, the same time frame used in mass combat."),
            },
            Topic::SocialMonologue => Entry {
                term: "Monologue / Study (3/-2)",
                what: "Social combat's version of Aim: builds toward a stronger social attack, either as an ongoing speech (Monologue) or aimed at one specific target (Study).",
                interacts: "Carries a steeper DV penalty than physical Aim (-2, not -1) because a monologue leaves the speaker more exposed than a combat feint does.",
                source: book_unquoted(171),
            },
            Topic::SocialAttack => Entry {
                term: "Social Attack (by Ability/-2)",
                what: "A push against someone's Mental Defense Value, using Presence, Investigation, or Performance.",
                interacts: "Speed and Rate are set by the Ability used: Presence is Speed 4, Rate 2; Investigation is Speed 5, Rate 2; Performance is Speed 6, Rate 1. Presence and Investigation each reach a single target (a person or one organized social unit); Performance reaches everyone who can perceive it, with no way to exclude anyone.",
                source: book_range_unquoted(171, 172),
            },
            Topic::SocialDash => Entry {
                term: "Dash (3/-3)",
                what: "A social combat sprint away from the exchange \u{2014} disengaging attention rather than covering ground.",
                interacts: "Carries a steeper DV penalty than physical Dash (-3, not -2), and like its physical counterpart cannot be parried at all without a stunt or magic.",
                source: book_unquoted(171),
            },
            Topic::SocialInactive => Entry {
                term: "Inactive (3/Special)",
                what: "Not participating in the exchange at all \u{2014} distracted, unconscious, or otherwise unable to engage socially.",
                interacts: "Unlike physical Inactive (DV 0, wide open), the book runs this the other way: being unreachable for conversation makes a character socially invulnerable rather than defenseless, since there's no way to argue with someone who can't hear you. The Speed/refresh shape otherwise follows the standard Inactive action.",
                source: book(171, "while unconsciousness makes characters physically vulnerable, such a state generally serves to make them socially invulnerable by making it impossible to communicate with them"),
            },
            Topic::SocialMiscellaneous => Entry {
                term: "Miscellaneous Action (5/-2)",
                what: "Anything social combat's other named actions don't cover.",
                interacts: "Unlike physical combat, fully concentrating on a miscellaneous action here doesn't zero MDV \u{2014} it grants social invulnerability, as if inactive. The -2 default (rather than physical combat's -1) is one eye on the exchange; the app models only that choice, not full concentration's different effect.",
                source: book_unquoted(171),
            },
            Topic::SocialFlurry => Entry {
                term: "Flurry (varies/varies)",
                what: "Several social actions declared together on one tick.",
                interacts: "The default here (Speed 4, DV -4) models two Presence attacks flurried together \u{2014} an app convention for the common case, not a fixed book value; Speed is still the highest Speed among the flurried actions and each still applies its own DV penalty, cumulatively.",
                source: Source::AppConvention,
            },
            Topic::ActionJoinDebateInProgress => Entry {
                term: "Join Debate, in progress (varies/-0)",
                what: "How a debater joins a social exchange that has already started.",
                interacts: "Same underlying formula as Join Battle in progress \u{2014} (frozen reaction count \u{2212} Wits + Awareness successes), clamped to 0\u{2013}6 \u{2014} just measured in long ticks. The book nests Join Debate under the Speed-5 Miscellaneous Action heading rather than restating the formula, so the app's input defaults to 5, distinct from personal combat's rolled default of 0.",
                source: book_unquoted(171),
            },
            Topic::ActionReadMotivation => Entry {
                term: "Read Motivation (5/varies)",
                what: "Studies someone across five long ticks to learn what drives them.",
                interacts: "The book never states a DV penalty for this action; the app defaults it to 0 rather than inventing one the rules don't specify.",
                source: book_unquoted(171),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use exalted_battle_wheel::battle::catalog;

    const ALL: &[Topic] = &[
        Topic::AppOverview,
        Topic::Undo,
        Topic::Redo,
        Topic::EventLog,
        Topic::CurrentTick,
        Topic::AdvanceTick,
        Topic::TeachingMode,
        Topic::Theme,
        Topic::Reset,
        Topic::ReactionCount,
        Topic::Room,
        Topic::RoomAdminModel,
        Topic::RoomRoleHost,
        Topic::RoomRoleAdmin,
        Topic::RoomRoleSpectator,
        Topic::RoomAdmin,
        Topic::RoomRename,
        Topic::RoomCopyCode,
        Topic::RoomKick,
        Topic::RoomStunServers,
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
        Topic::DvPenaltyRing,
        Topic::DvPenaltyFloor,
        Topic::SectorCountdown,
        Topic::MarkerGutter,
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
        Topic::BattleModeSelect,
        Topic::LongTick,
        Topic::ActionCoordinateAttacks,
        Topic::ActionReadyWeapons,
        Topic::ActionRiseFromProne,
        Topic::ActionJump,
        Topic::JoinWar,
        Topic::ActionChangeFormation,
        Topic::ActionDisengage,
        Topic::ActionTurn,
        Topic::ActionSplitUnit,
        Topic::ActionExpelSpecialCharacter,
        Topic::ActionMergeUnits,
        Topic::ActionSignalUnits,
        Topic::ActionRally,
        Topic::JoinDebate,
        Topic::SocialMonologue,
        Topic::SocialAttack,
        Topic::SocialDash,
        Topic::SocialInactive,
        Topic::SocialMiscellaneous,
        Topic::SocialFlurry,
        Topic::ActionJoinDebateInProgress,
        Topic::ActionReadMotivation,
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

    /// Closes the half of the guarantee `topics_for`'s exhaustive match can't give at compile
    /// time: that every action actually wired into a mode's real catalog resolves to a topic.
    /// Walks `catalog(mode)` rather than a hand-maintained list, so it can't be satisfied by a
    /// stale or over-broad `ModeTopics` entry.
    #[test]
    fn every_catalog_action_has_a_topic() {
        for mode in BattleMode::ALL {
            for template in catalog(mode) {
                let topic = action_topic(mode, template.kind)
                    .unwrap_or_else(|| panic!("{:?} is in the {mode:?} catalog with no glossary topic", template.kind));
                let entry = topic.entry();
                assert!(!entry.what.is_empty(), "{topic:?} has an empty `what`");
            }
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
