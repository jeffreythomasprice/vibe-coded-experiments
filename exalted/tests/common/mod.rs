//! Shared test helpers. Build a fully-valid Solar character we can mutate
//! per-test to exercise specific rules.

use exalted::Character;
use exalted::character::{
    AbilityKind, AttributeGroup, AttributeKind, AttributePriority, BackgroundKind, BackgroundRef,
    Caste, CharmRef, DotSource, Intimacy, IntimacyKind, KnownLanguage, LanguageFamily, RatedTrait,
    VirtueKind,
};
use exalted::character::identity::VirtueFlaw;

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
    add_chargen(c.attributes.get_mut(&AttributeKind::Manipulation).unwrap(), 2);
    add_chargen(c.attributes.get_mut(&AttributeKind::Appearance).unwrap(), 2);
    // Mental: 4 → Per+2, Int+1, Wits+1
    add_chargen(c.attributes.get_mut(&AttributeKind::Perception).unwrap(), 2);
    add_chargen(c.attributes.get_mut(&AttributeKind::Intelligence).unwrap(), 1);
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
    c.backgrounds
        .push(BackgroundRef::lookup_kind(BackgroundKind::Resources, resources));

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
    c.backgrounds
        .push(BackgroundRef::lookup_kind(BackgroundKind::Contacts, contacts));

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

fn add_chargen(t: &mut RatedTrait, n: usize) {
    for _ in 0..n {
        t.add_chargen();
    }
}

fn add_bp(t: &mut RatedTrait, spent: u8) {
    t.add_bonus(spent);
}
