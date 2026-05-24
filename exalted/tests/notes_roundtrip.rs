//! Round-trip a character whose Notes appear in every supported location
//! (Character, Background, Charm, Spell, Combo) through TOML serialization
//! and deserialization, asserting nothing is lost.

mod common;

use common::valid_dawn_with_notes_demo;
use exalted::character::{BackgroundRef, CharmRef, SpellRef};

#[test]
fn notes_round_trip_through_toml_on_every_entity() {
    let original = valid_dawn_with_notes_demo();

    // Pre-conditions: the demo fixture should populate notes everywhere we
    // claim it does. If this ever drifts, the test below would silently
    // pass even with broken serialization, so guard it explicitly.
    assert!(!original.notes.is_empty(), "Character.notes empty");
    assert!(
        original.backgrounds.iter().any(|b| !b.notes().is_empty()),
        "no background carries a note"
    );
    assert!(
        original.charms.iter().any(|c| !c.notes().is_empty()),
        "no charm carries a note"
    );
    assert!(
        original.spells.iter().any(|s| !s.notes().is_empty()),
        "no spell carries a note"
    );
    assert!(
        original.combos.iter().any(|c| !c.notes.is_empty()),
        "no combo carries a note"
    );

    let text = toml::to_string_pretty(&original).expect("serialize");
    let decoded: exalted::Character = toml::from_str(&text).expect("deserialize");

    assert_eq!(original, decoded, "round-trip mutated the character");

    // Also assert the notes survived intact at each location specifically —
    // catches a regression where the outer equality holds but variant-specific
    // accessors stop seeing the notes.
    assert_eq!(original.notes, decoded.notes);
    for (a, b) in original.charms.iter().zip(&decoded.charms) {
        assert_eq!(charm_notes(a), charm_notes(b));
    }
    for (a, b) in original.backgrounds.iter().zip(&decoded.backgrounds) {
        assert_eq!(background_notes(a), background_notes(b));
    }
    for (a, b) in original.spells.iter().zip(&decoded.spells) {
        assert_eq!(spell_notes(a), spell_notes(b));
    }
    for (a, b) in original.combos.iter().zip(&decoded.combos) {
        assert_eq!(a.notes, b.notes);
    }
}

fn charm_notes(c: &CharmRef) -> &[exalted::character::Note] {
    c.notes()
}

fn background_notes(b: &BackgroundRef) -> &[exalted::character::Note] {
    b.notes()
}

fn spell_notes(s: &SpellRef) -> &[exalted::character::Note] {
    s.notes()
}
