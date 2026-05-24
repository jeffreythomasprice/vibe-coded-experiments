//! Shared test helpers. Build a fully-valid Solar character we can mutate
//! per-test to exercise specific rules.

use chrono::{DateTime, Utc};
use exalted::Character;
use exalted::character::identity::VirtueFlaw;
use exalted::character::{
    AbilityKind, AttributeGroup, AttributeKind, AttributePriority, BackgroundKind, BackgroundRef,
    Caste, CharmRef, Combo, DotSource, Intimacy, IntimacyKind, KnownLanguage, LanguageFamily, Note,
    RatedTrait, SpellRef, VirtueKind, XpAward,
};

/// A canonical valid Solar Dawn character used as the baseline for chargen
/// tests. 15 BP spent, every chargen pool exactly filled.
pub fn valid_dawn() -> Character {
    let mut c = Character::new_blank_solar("Test Solar", Caste::Dawn);

    // Favored abilities (must not overlap with Dawn caste).
    c.favored_abilities = vec![
        AbilityKind::Awareness,
        AbilityKind::Dodge,
        AbilityKind::Stealth,
        AbilityKind::Survival, // Zenith, not Dawn — OK
        AbilityKind::Athletics,
    ];

    // Attribute priority: Physical 8, Social 6, Mental 4.
    c.attribute_priority = AttributePriority {
        primary: AttributeGroup::Physical,
        secondary: AttributeGroup::Social,
        tertiary: AttributeGroup::Mental,
    };

    // Physical: 8 → Str+3, Dex+3, Sta+2 (each starts at 1).
    add_chargen(c.attributes.get_mut(&AttributeKind::Strength).unwrap(), 3);
    add_chargen(c.attributes.get_mut(&AttributeKind::Dexterity).unwrap(), 3);
    add_chargen(c.attributes.get_mut(&AttributeKind::Stamina).unwrap(), 2);
    // Social: 6 → Cha+2, Manip+2, App+2
    add_chargen(c.attributes.get_mut(&AttributeKind::Charisma).unwrap(), 2);
    add_chargen(
        c.attributes.get_mut(&AttributeKind::Manipulation).unwrap(),
        2,
    );
    add_chargen(c.attributes.get_mut(&AttributeKind::Appearance).unwrap(), 2);
    // Mental: 4 → Per+2, Int+1, Wits+1
    add_chargen(c.attributes.get_mut(&AttributeKind::Perception).unwrap(), 2);
    add_chargen(
        c.attributes.get_mut(&AttributeKind::Intelligence).unwrap(),
        1,
    );
    add_chargen(c.attributes.get_mut(&AttributeKind::Wits).unwrap(), 1);

    // Abilities: 28 chargen dots. 5 favored × 3 = 15, 5 caste × 1 = 5
    // (20 in C/F), 8 more in non-C/F abilities.
    for fav in [
        AbilityKind::Awareness,
        AbilityKind::Dodge,
        AbilityKind::Stealth,
        AbilityKind::Survival,
        AbilityKind::Athletics,
    ] {
        add_chargen(c.abilities.get_mut(&fav).unwrap(), 3);
    }
    for caste in [
        AbilityKind::Archery,
        AbilityKind::MartialArts,
        AbilityKind::Melee,
        AbilityKind::Thrown,
        AbilityKind::War,
    ] {
        add_chargen(c.abilities.get_mut(&caste).unwrap(), 1);
    }
    add_chargen(c.abilities.get_mut(&AbilityKind::Lore).unwrap(), 3);
    add_chargen(c.abilities.get_mut(&AbilityKind::Occult).unwrap(), 3);
    add_chargen(c.abilities.get_mut(&AbilityKind::Investigation).unwrap(), 2);

    // Virtues: 5 chargen dots. Compassion +2, Conviction +2, Valor +1.
    add_chargen(c.virtues.get_mut(&VirtueKind::Compassion).unwrap(), 2);
    add_chargen(c.virtues.get_mut(&VirtueKind::Conviction).unwrap(), 2);
    add_chargen(c.virtues.get_mut(&VirtueKind::Valor).unwrap(), 1);
    c.primary_virtue = Some(VirtueKind::Compassion);
    c.virtue_flaw = Some(VirtueFlaw::CompassionateMartyrdom);

    // Backgrounds: 7 chargen dots, ≤3 each. Add BP-purchased extras after the
    // initial chargen allocation (Resources +1 BP, Mentor +1 BP, Contacts +1 BP).
    let mut resources = RatedTrait::with_base(0);
    for _ in 0..3 {
        resources.add_chargen();
    }
    resources.add_bonus(2);
    c.backgrounds.push(BackgroundRef::lookup_kind(
        BackgroundKind::Resources,
        resources,
    ));

    let mut mentor = RatedTrait::with_base(0);
    for _ in 0..2 {
        mentor.add_chargen();
    }
    mentor.add_bonus(1);
    c.backgrounds
        .push(BackgroundRef::lookup_kind(BackgroundKind::Mentor, mentor));

    let mut contacts = RatedTrait::with_base(0);
    for _ in 0..2 {
        contacts.add_chargen();
    }
    contacts.add_bonus(1);
    c.backgrounds.push(BackgroundRef::lookup_kind(
        BackgroundKind::Contacts,
        contacts,
    ));

    // 10 charms: 5 favored excellencies + 5 caste excellencies.
    c.charms = vec![
        CharmRef::lookup("first-awareness-excellency", DotSource::ChargenPriority),
        CharmRef::lookup("first-dodge-excellency", DotSource::ChargenPriority),
        CharmRef::lookup("first-stealth-excellency", DotSource::ChargenPriority),
        CharmRef::lookup("first-survival-excellency", DotSource::ChargenPriority),
        CharmRef::lookup("first-athletics-excellency", DotSource::ChargenPriority),
        CharmRef::lookup("first-archery-excellency", DotSource::ChargenPriority),
        CharmRef::lookup("first-martial-arts-excellency", DotSource::ChargenPriority),
        CharmRef::lookup("first-melee-excellency", DotSource::ChargenPriority),
        CharmRef::lookup("first-thrown-excellency", DotSource::ChargenPriority),
        CharmRef::lookup("first-war-excellency", DotSource::ChargenPriority),
    ];

    // Intimacies: Compassion-baseline (3).
    for desc in ["The downtrodden", "My dojo", "Honor"] {
        c.intimacies.push(Intimacy {
            description: desc.to_string(),
            kind: IntimacyKind::Cause,
            source: DotSource::Base,
            rating: 1,
        });
    }

    // Willpower base = sum of two highest virtues = Compassion(3) + Conviction(3) = 6.
    c.willpower = RatedTrait::with_base(6);

    // 15 BP spent:
    //   - Dex chargen-priority took it to 4. Add BP to reach 5: 4 BP
    //   - Melee 1→2 (caste): 1 BP
    //   - Awareness 3→4 (favored): 1 BP
    //   - Dodge 3→4 (favored): 1 BP
    //   - Resources 3→4 (above 3, 2 BP): 2 BP
    //   - Mentor 2→3 (≤3, 1 BP): 1 BP
    //   - Contacts 2→3: 1 BP
    //   - Extra Charm (caste): 4 BP
    add_bp(c.attributes.get_mut(&AttributeKind::Dexterity).unwrap(), 4);
    add_bp(c.abilities.get_mut(&AbilityKind::Melee).unwrap(), 1);
    add_bp(c.abilities.get_mut(&AbilityKind::Awareness).unwrap(), 1);
    add_bp(c.abilities.get_mut(&AbilityKind::Dodge).unwrap(), 1);
    c.charms.push(CharmRef::lookup(
        "second-martial-arts-excellency",
        DotSource::BonusPoints { spent: 4 },
    ));

    // Linguistics 0 → just the free native language family.
    c.languages = vec![KnownLanguage {
        family: LanguageFamily::Riverspeak,
        dialect_specialty: Some("Nexus".to_string()),
        native: true,
    }];

    c
}

/// Build the canonical sample character used by `assets/sample-character.toml`.
#[allow(dead_code)]
/// Extends [`valid_dawn`] with post-chargen XP purchases (an Essence boost,
/// the Terrestrial Circle Sorcery Charm, a Terrestrial spell, and a Combo)
/// plus pinned-timestamp notes on every supported location: the Character,
/// one Background, one Charm, the spell, the Combo, and a top-level journal.
/// Existing in-game rules require the sorcery Charm + Essence 3 before a
/// spell can be learnt, hence the 46 XP outlay.
pub fn valid_dawn_with_notes_demo() -> Character {
    exalted::rules::database::init_database().ok();
    let mut c = valid_dawn();

    // Attach a note to one Charm in place. Pulled out as a helper so the
    // `if let` to reach inside the variant stays compact.
    push_note_on_charm(
        &mut c,
        "first-awareness-excellency",
        fixed_note(
            "Reflexive — use to detect ambushes during downtime travel.",
            "2026-04-18T20:15:00Z",
            "2026-05-02T19:30:00Z",
        ),
    );

    // Attach a note to the Mentor background.
    push_note_on_background(
        &mut c,
        BackgroundKind::Mentor,
        fixed_note(
            "Old Sifu Wen, master of the Silken Reed style. Lives in Nexus.",
            "2026-03-12T09:00:00Z",
            "2026-03-12T09:00:00Z",
        ),
    );

    // Essence 2 → 3 (24 XP), Terrestrial Circle Sorcery (10 XP, Occult is
    // non-C/F here), Blood Lash spell (10 XP), and the Watchful Step Combo
    // bundling First Awareness + First Dodge (2 XP, sum of member min
    // Ability ratings). 46 XP earned, 0 banked.
    c.essence.add_xp(24);
    c.charms.push(CharmRef::lookup(
        "terrestrial-circle-sorcery",
        DotSource::Xp { spent: 10 },
    ));

    let mut spell = SpellRef::lookup("blood-lash", DotSource::Xp { spent: 10 });
    if let SpellRef::Lookup { notes, .. } = &mut spell {
        notes.push(fixed_note(
            "Learnt from a Nexus binder during the Festival of Waters.",
            "2026-04-30T22:10:00Z",
            "2026-04-30T22:10:00Z",
        ));
    }
    c.spells.push(spell);

    c.combos.push(Combo {
        name: "Watchful Step".to_string(),
        charm_ids: vec![
            "first-awareness-excellency".to_string(),
            "first-dodge-excellency".to_string(),
        ],
        source: DotSource::Xp { spent: 2 },
        notes: vec![fixed_note(
            "Default opener when ambushed in alleys — boost JB then dodge.",
            "2026-05-04T18:45:00Z",
            "2026-05-09T12:00:00Z",
        )],
    });

    c.xp_earned = 46;
    c.xp_banked = 0;
    c.xp_awards = vec![
        fixed_award(
            8,
            "Session 1 (Firewander orphans): 4 base + 2 stunt + 1 training + 1 RP",
            "2026-04-12T23:30:00Z",
            "2026-04-12T23:30:00Z",
        ),
        fixed_award(
            7,
            "Session 2 (dockside ambush): 4 base + 2 dramatic + 1 training",
            "2026-04-19T23:30:00Z",
            "2026-04-19T23:30:00Z",
        ),
        fixed_award(
            6,
            "Downtime arc — studying the Silken Reed manuals under Sifu Wen",
            "2026-04-26T19:00:00Z",
            "2026-04-26T19:00:00Z",
        ),
        fixed_award(
            10,
            "Session 3 (Festival of Waters): tracked the Nexus binder; bonus for learning Blood Lash in fiction",
            "2026-04-30T23:30:00Z",
            "2026-05-02T17:10:00Z",
        ),
        fixed_award(
            9,
            "Session 4 (Guild factor confrontation): 4 base + 2 stunt + 2 dramatic + 1 RP",
            "2026-05-09T23:30:00Z",
            "2026-05-09T23:30:00Z",
        ),
        fixed_award(
            6,
            "Session 5 (downtime, dojo reconstruction): 4 base + 2 training",
            "2026-05-16T23:30:00Z",
            "2026-05-16T23:30:00Z",
        ),
    ];

    c.notes = vec![
        fixed_note(
            "Session 1: party rescued the river-orphans of Nexus's Firewander District.",
            "2026-04-12T23:30:00Z",
            "2026-04-12T23:30:00Z",
        ),
        fixed_note(
            "Suspect the Guild factor is laundering jade through the dockside cult.",
            "2026-05-04T20:00:00Z",
            "2026-05-11T19:20:00Z",
        ),
    ];

    c
}

fn push_note_on_charm(c: &mut Character, id: &str, note: Note) {
    let charm = c
        .charms
        .iter_mut()
        .find(|ch| ch.is_id(id))
        .unwrap_or_else(|| panic!("charm {id} not in baseline fixture"));
    match charm {
        CharmRef::Lookup { notes, .. } | CharmRef::Custom { notes, .. } => notes.push(note),
    }
}

fn push_note_on_background(c: &mut Character, kind: BackgroundKind, note: Note) {
    let db = exalted::rules::database::database();
    let bg = c
        .backgrounds
        .iter_mut()
        .find(|b| b.kind(db) == Some(kind))
        .unwrap_or_else(|| panic!("no {kind:?} background in baseline fixture"));
    match bg {
        BackgroundRef::Lookup { notes, .. } | BackgroundRef::Custom { notes, .. } => {
            notes.push(note)
        }
    }
}

/// Build a `Note` with explicit RFC3339 timestamps so the regenerated
/// `sample-character.toml` is byte-stable. Panics if the strings can't be
/// parsed (test-only helper).
fn fixed_note(body: &str, created_rfc3339: &str, updated_rfc3339: &str) -> Note {
    Note {
        body: body.to_string(),
        created_at: parse_ts(created_rfc3339),
        updated_at: parse_ts(updated_rfc3339),
    }
}

/// Same idea as `fixed_note`, but for `XpAward`. Keeps the sample fixture
/// byte-stable across runs.
fn fixed_award(amount: u32, body: &str, created_rfc3339: &str, updated_rfc3339: &str) -> XpAward {
    XpAward {
        amount,
        body: body.to_string(),
        created_at: parse_ts(created_rfc3339),
        updated_at: parse_ts(updated_rfc3339),
    }
}

fn parse_ts(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .unwrap_or_else(|e| panic!("bad fixed timestamp {s:?}: {e}"))
        .with_timezone(&Utc)
}

fn add_chargen(t: &mut RatedTrait, n: usize) {
    for _ in 0..n {
        t.add_chargen();
    }
}

fn add_bp(t: &mut RatedTrait, spent: u8) {
    t.add_bonus(spent);
}
