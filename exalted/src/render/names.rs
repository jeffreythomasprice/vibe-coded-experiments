//! Shared display-name helpers used by every renderer (markdown, pdf, ...).
//!
//! These are pure lookups from typed enum variants to canonical display
//! strings. They don't depend on any rendering format.

use crate::character::{
    AbilityKind, AttributeGroup, AttributeKind, Caste, DotSource, IntimacyKind, SpellCircle,
    VirtueKind,
};

pub(crate) fn caste_name(caste: Caste) -> &'static str {
    match caste {
        Caste::Dawn => "Dawn",
        Caste::Zenith => "Zenith",
        Caste::Twilight => "Twilight",
        Caste::Night => "Night",
        Caste::Eclipse => "Eclipse",
    }
}

pub(crate) fn group_name(group: AttributeGroup) -> &'static str {
    match group {
        AttributeGroup::Physical => "Physical",
        AttributeGroup::Social => "Social",
        AttributeGroup::Mental => "Mental",
    }
}

pub(crate) fn attr_name(kind: AttributeKind) -> &'static str {
    match kind {
        AttributeKind::Strength => "Strength",
        AttributeKind::Dexterity => "Dexterity",
        AttributeKind::Stamina => "Stamina",
        AttributeKind::Charisma => "Charisma",
        AttributeKind::Manipulation => "Manipulation",
        AttributeKind::Appearance => "Appearance",
        AttributeKind::Perception => "Perception",
        AttributeKind::Intelligence => "Intelligence",
        AttributeKind::Wits => "Wits",
    }
}

pub(crate) fn ability_name(kind: AbilityKind) -> &'static str {
    match kind {
        AbilityKind::Archery => "Archery",
        AbilityKind::MartialArts => "Martial Arts",
        AbilityKind::Melee => "Melee",
        AbilityKind::Thrown => "Thrown",
        AbilityKind::War => "War",
        AbilityKind::Integrity => "Integrity",
        AbilityKind::Performance => "Performance",
        AbilityKind::Presence => "Presence",
        AbilityKind::Resistance => "Resistance",
        AbilityKind::Survival => "Survival",
        AbilityKind::Craft => "Craft",
        AbilityKind::Investigation => "Investigation",
        AbilityKind::Lore => "Lore",
        AbilityKind::Medicine => "Medicine",
        AbilityKind::Occult => "Occult",
        AbilityKind::Athletics => "Athletics",
        AbilityKind::Awareness => "Awareness",
        AbilityKind::Dodge => "Dodge",
        AbilityKind::Larceny => "Larceny",
        AbilityKind::Stealth => "Stealth",
        AbilityKind::Bureaucracy => "Bureaucracy",
        AbilityKind::Linguistics => "Linguistics",
        AbilityKind::Ride => "Ride",
        AbilityKind::Sail => "Sail",
        AbilityKind::Socialize => "Socialize",
    }
}

pub(crate) fn virtue_name(kind: VirtueKind) -> &'static str {
    match kind {
        VirtueKind::Compassion => "Compassion",
        VirtueKind::Conviction => "Conviction",
        VirtueKind::Temperance => "Temperance",
        VirtueKind::Valor => "Valor",
    }
}

pub(crate) fn source_tag(source: DotSource) -> &'static str {
    match source {
        DotSource::Base => "base",
        DotSource::ChargenPriority => "chargen",
        DotSource::BonusPoints { .. } => "bp",
        DotSource::Xp { .. } => "xp",
    }
}

pub(crate) fn spell_circle_label(c: SpellCircle) -> &'static str {
    match c {
        SpellCircle::Terrestrial => "Terrestrial",
        SpellCircle::Celestial => "Celestial",
        SpellCircle::Solar => "Solar",
        SpellCircle::Shadowlands => "Shadowlands",
        SpellCircle::Void => "Void",
        SpellCircle::Labyrinth => "Labyrinth",
    }
}

pub(crate) fn intimacy_kind(k: IntimacyKind) -> &'static str {
    match k {
        IntimacyKind::Person => "Person",
        IntimacyKind::Place => "Place",
        IntimacyKind::Cause => "Cause",
        IntimacyKind::Ideal => "Ideal",
        IntimacyKind::Other => "Other",
    }
}
