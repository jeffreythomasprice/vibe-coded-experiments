//! Build the rulebook's example character — Abu Nuwas, the Night-Caste spy
//! from Chiaroscuro — step by step from the Exalted 2E walkthrough on
//! pp. 82–85, then assert the chargen validator accepts him.
//!
//! Two notes about the source:
//! - The rulebook prose places 27 ability dots when summed strictly, while
//!   asserting a total of 28; one extra dot has been added to Stealth (the
//!   ability where the prose-vs-totals discrepancy is most plausible).
//! - The rulebook gives Ox-Body Technique as `+2 × -1` levels, which is one
//!   of three valid Ox-Body patterns. The current `health::ox_body` model
//!   uses a fixed `+1 × -0 + 2 × -2` pattern. The chargen validator does
//!   not assert on health track shape, so this divergence does not affect
//!   the test.

use exalted::Character;
use exalted::character::{
    AbilityKind, Anima, Appearance, AttributeGroup, AttributeKind, AttributePriority,
    BackgroundKind, Caste, ChosenCharm, DotSource, Identity, Intimacy, IntimacyKind,
    RatedTrait, Specialty, VirtueKind,
};
use exalted::character::identity::VirtueFlaw;

fn add_chargen(t: &mut RatedTrait, n: usize) {
    for _ in 0..n {
        t.add_chargen();
    }
}

fn add_bp(t: &mut RatedTrait, spent: u8) {
    t.add_bonus(spent);
}

fn abu_nuwas() -> Character {
    let mut c = Character::new_blank_solar("Abu Nuwas", Caste::Night);

    c.identity = Identity {
        name: "Abu Nuwas".to_string(),
        concept: "Diplomat and spy from Chiaroscuro".to_string(),
        motivation: "Protect the common people".to_string(),
        anima: Anima {
            totem: "Desert owl, ghostly white with glowing purple eyes".to_string(),
            iconic_image: "A swift and silent desert owl".to_string(),
        },
        appearance: Appearance {
            hair: "Black".to_string(),
            eyes: "Dark".to_string(),
            skin: "Dark olive (Delzahn)".to_string(),
            distinguishing_features: "Exceptionally average looking".to_string(),
            homeland: "Chiaroscuro".to_string(),
            sex: "Male".to_string(),
            age: Some(25),
        },
        player: "Charles".to_string(),
        chronicle: "Southern Politics".to_string(),
    };

    // Favored abilities: Socialize, Bureaucracy, Resistance, Investigation, Thrown.
    c.favored_abilities = vec![
        AbilityKind::Socialize,
        AbilityKind::Bureaucracy,
        AbilityKind::Resistance,
        AbilityKind::Investigation,
        AbilityKind::Thrown,
    ];

    // Attribute priority: Mental primary (8), Physical secondary (6), Social tertiary (4).
    c.attribute_priority = AttributePriority {
        primary: AttributeGroup::Mental,
        secondary: AttributeGroup::Physical,
        tertiary: AttributeGroup::Social,
    };

    // Mental: 8 dots. Perception 4 (=1+3), Intelligence 3 (=1+2), Wits 4 (=1+3).
    add_chargen(c.attributes.get_mut(&AttributeKind::Perception).unwrap(), 3);
    add_chargen(c.attributes.get_mut(&AttributeKind::Intelligence).unwrap(), 2);
    add_chargen(c.attributes.get_mut(&AttributeKind::Wits).unwrap(), 3);

    // Physical: 6 dots. Strength 2 (=1+1), Dexterity 4 (=1+3), Stamina 3 (=1+2).
    add_chargen(c.attributes.get_mut(&AttributeKind::Strength).unwrap(), 1);
    add_chargen(c.attributes.get_mut(&AttributeKind::Dexterity).unwrap(), 3);
    add_chargen(c.attributes.get_mut(&AttributeKind::Stamina).unwrap(), 2);

    // Social: 4 dots + 1 BP for Charisma. Appearance 2 (=1+1), Manipulation 3
    // (=1+2), Charisma 3 (=1+1 chargen + 1 BP for 4 BP).
    add_chargen(c.attributes.get_mut(&AttributeKind::Appearance).unwrap(), 1);
    add_chargen(c.attributes.get_mut(&AttributeKind::Manipulation).unwrap(), 2);
    add_chargen(c.attributes.get_mut(&AttributeKind::Charisma).unwrap(), 1);
    add_bp(c.attributes.get_mut(&AttributeKind::Charisma).unwrap(), 4);

    // Abilities: 28 chargen dots.
    // Each favored gets ≥1 → Socialize, Bureaucracy, Resistance get 1 each.
    add_chargen(c.abilities.get_mut(&AbilityKind::Socialize).unwrap(), 1);
    add_chargen(c.abilities.get_mut(&AbilityKind::Bureaucracy).unwrap(), 1);
    add_chargen(c.abilities.get_mut(&AbilityKind::Resistance).unwrap(), 1);
    // Investigation: 1 favored + 2 more = 3 dots
    add_chargen(c.abilities.get_mut(&AbilityKind::Investigation).unwrap(), 3);
    // Thrown: 1 favored + 2 more = 3 dots
    add_chargen(c.abilities.get_mut(&AbilityKind::Thrown).unwrap(), 3);
    // Caste abilities (Night): Athletics, Awareness, Dodge, Larceny, Stealth.
    add_chargen(c.abilities.get_mut(&AbilityKind::Athletics).unwrap(), 1);
    add_chargen(c.abilities.get_mut(&AbilityKind::Awareness).unwrap(), 3);
    add_chargen(c.abilities.get_mut(&AbilityKind::Dodge).unwrap(), 3);
    add_chargen(c.abilities.get_mut(&AbilityKind::Larceny).unwrap(), 3);
    // Stealth gets 3 instead of the prose's "two dots" — the prose appears
    // to undercount by one against its stated 28-dot total.
    add_chargen(c.abilities.get_mut(&AbilityKind::Stealth).unwrap(), 3);
    // Non-C/F abilities: Linguistics 3, Ride 1, Melee 1, Medicine 1.
    add_chargen(c.abilities.get_mut(&AbilityKind::Linguistics).unwrap(), 3);
    add_chargen(c.abilities.get_mut(&AbilityKind::Ride).unwrap(), 1);
    add_chargen(c.abilities.get_mut(&AbilityKind::Melee).unwrap(), 1);
    add_chargen(c.abilities.get_mut(&AbilityKind::Medicine).unwrap(), 1);

    // Specialty: 1 BP on Awareness "Listening". Awareness is Caste, so 1 BP
    // buys 2 specialty entries (rulebook says "two dots in this specialty").
    {
        let awareness = c.abilities.get_mut(&AbilityKind::Awareness).unwrap();
        awareness.specialties.push(Specialty {
            name: "Listening".to_string(),
            source: DotSource::BonusPoints { spent: 1 },
        });
        awareness.specialties.push(Specialty {
            name: "Listening".to_string(),
            source: DotSource::BonusPoints { spent: 0 },
        });
    }

    // Virtues: Compassion 3 (+2), Conviction 3 (+2), Valor 2 (+1), Temperance 1.
    add_chargen(c.virtues.get_mut(&VirtueKind::Compassion).unwrap(), 2);
    add_chargen(c.virtues.get_mut(&VirtueKind::Conviction).unwrap(), 2);
    add_chargen(c.virtues.get_mut(&VirtueKind::Valor).unwrap(), 1);
    c.primary_virtue = VirtueKind::Compassion;
    c.virtue_flaw = VirtueFlaw::CompassionateMartyrdom;

    // Backgrounds: 7 chargen dots + Manse 4 from BP + Artifact 2 (1 chargen + 1 BP).
    add_chargen(c.backgrounds.get_mut(&BackgroundKind::Backing).unwrap(), 3);
    add_chargen(c.backgrounds.get_mut(&BackgroundKind::Resources).unwrap(), 3);
    add_chargen(c.backgrounds.get_mut(&BackgroundKind::Artifact).unwrap(), 1);
    add_bp(c.backgrounds.get_mut(&BackgroundKind::Artifact).unwrap(), 1);
    // Manse 4 from BP: 1+1+1+2 BP for dots 1,2,3,4.
    {
        let manse = c.backgrounds.get_mut(&BackgroundKind::Manse).unwrap();
        manse.add_bonus(1);
        manse.add_bonus(1);
        manse.add_bonus(1);
        manse.add_bonus(2);
    }

    // Charms: 10 from chargen + 1 from BP (Flawless Pickpocketing — 4 BP, Caste).
    c.charms = vec![
        ChosenCharm::new("Shadow Over Water", DotSource::ChargenPriority),
        ChosenCharm::new("Second Dodge Excellency", DotSource::ChargenPriority),
        ChosenCharm::new("Body-Mending Meditation", DotSource::ChargenPriority),
        ChosenCharm::new("Ox-Body Technique", DotSource::ChargenPriority),
        ChosenCharm::new("First Awareness Excellency", DotSource::ChargenPriority),
        ChosenCharm::new("Keen Hearing and Touch Technique", DotSource::ChargenPriority),
        ChosenCharm::new("Second Stealth Excellency", DotSource::ChargenPriority),
        ChosenCharm::new("Graceful Crane Stance", DotSource::ChargenPriority),
        ChosenCharm::new("Second Socialize Excellency", DotSource::ChargenPriority),
        ChosenCharm::new("Lock-Opening Touch", DotSource::ChargenPriority),
        ChosenCharm::new(
            "Flawless Pickpocketing Technique",
            DotSource::BonusPoints { spent: 4 },
        ),
    ];

    // Intimacies (3 — matches Compassion baseline).
    for desc in [
        "Chiaroscuro (home nation)",
        "Living the High Life",
        "Hatred of the Realm",
    ] {
        c.intimacies.push(Intimacy {
            description: desc.to_string(),
            kind: IntimacyKind::Cause,
            source: DotSource::Base,
        });
    }

    // Willpower base = Compassion(3) + Conviction(3) = 6.
    c.willpower = RatedTrait::with_base(6);

    // Essence stays at 2.
    c
}

#[test]
fn abu_nuwas_validates_at_chargen() {
    let abu = abu_nuwas();
    let report = abu.validate_chargen();
    assert!(
        report.is_ok(),
        "Abu Nuwas should validate at chargen — errors: {:#?}",
        report.errors
    );
}

#[test]
fn abu_nuwas_bp_total_is_fifteen() {
    let abu = abu_nuwas();
    let bp = exalted::character::xp::total_bp_spent(&abu);
    assert_eq!(
        bp, 15,
        "Abu Nuwas should have spent exactly 15 bonus points"
    );
}

#[test]
fn abu_nuwas_essence_pools_match_walkthrough() {
    // The walkthrough explicitly computes Personal 12, Peripheral 29.
    let abu = abu_nuwas();
    assert_eq!(exalted::rules::personal_essence_max(&abu), 12);
    assert_eq!(exalted::rules::peripheral_essence_max(&abu), 29);
}

#[test]
fn abu_nuwas_attribute_summary() {
    // Per the walkthrough's explicit summaries.
    let abu = abu_nuwas();
    assert_eq!(abu.attribute(AttributeKind::Strength), 2);
    assert_eq!(abu.attribute(AttributeKind::Dexterity), 4);
    assert_eq!(abu.attribute(AttributeKind::Stamina), 3);
    assert_eq!(abu.attribute(AttributeKind::Charisma), 3);
    assert_eq!(abu.attribute(AttributeKind::Manipulation), 3);
    assert_eq!(abu.attribute(AttributeKind::Appearance), 2);
    assert_eq!(abu.attribute(AttributeKind::Perception), 4);
    assert_eq!(abu.attribute(AttributeKind::Intelligence), 3);
    assert_eq!(abu.attribute(AttributeKind::Wits), 4);
}

#[test]
fn abu_nuwas_willpower_six() {
    let abu = abu_nuwas();
    assert_eq!(abu.willpower_dots(), 6);
}

#[test]
fn abu_nuwas_charm_count_eleven() {
    let abu = abu_nuwas();
    assert_eq!(abu.charms.len(), 11);
    let chargen_count = abu
        .charms
        .iter()
        .filter(|c| c.source.is_chargen_priority())
        .count();
    assert_eq!(chargen_count, 10);
}
