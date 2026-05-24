//! Frozen mapping from semantic trait positions to AcroForm field names in
//! MrGone's editable 2e Solars sheet.
//!
//! Field names on this sheet fall into two camps:
//!
//! - **Generic dots** (`dotN`, 235 total on page 1) used for attribute,
//!   ability, and background rating dots. Numbering is not semantic; the
//!   mapping was derived by running the `dump_dots` example and clustering
//!   widgets by `/Rect` coordinates into rows and columns.
//! - **Named-prefix groups** (`willdot`, `essencedot`, `virtuecheck`,
//!   `LBCheck`, `healthcheck`, `skillscheck`, `willcheck`, `EPCheck`,
//!   `APCheck`, `xtdot`) used for everything that has dedicated visual
//!   structure: willpower track, essence rating, virtue dots, virtue
//!   channels, limit, health, caste/favored marks, willpower-temp,
//!   essence pools.
//!
//! ## Sheet layout (page 1)
//!
//! - Attributes (`dot1`–`dot45`): 3 columns × 3 rows × 5 dots, in
//!   Physical/Social/Mental groups. Each column's 15 dots are numbered top
//!   to bottom then dot-1 to dot-5 left to right, so e.g. column 1 reads
//!   Strength(1-5), Dexterity(6-10), Stamina(11-15).
//! - Abilities (`dot46`–`dot195`): 3 columns × 10 ability rows × 5 dots.
//!   30 ability slots total — the first 25 match
//!   [`crate::character::AbilityKind::ALL`]; slots 26–30 (`dot171`–`dot195`)
//!   are the five write-in specialty rows.
//! - Caste/favored marks (`skillscheck1`–`skillscheck30`): one tick box per
//!   ability slot, in the same column-major order as the ability dots —
//!   `skillscheck26`–`skillscheck30` correspond to the specialty rows.
//! - Backgrounds (`dot241`–`dot280`): 8 slots × 5 dots, single column.
//! - Virtues: rating dots in `virtuecheck1`–`virtuecheck20`
//!   (Compassion/Conviction on the top row, Temperance/Valor on the bottom,
//!   5 dots each); channels in `xtdot1`–`xtdot20` in the same layout.
//! - Willpower: rating in `willdot1`–`willdot10`, temp track in
//!   `willcheck1`–`willcheck10`.
//! - Essence: rating in `essencedot1`–`essencedot6` (sheet caps at 6 dots).
//! - Health: damage track in `healthcheck1`–`healthcheck22`.
//! - Limit: `LBCheck1`–`LBCheck10`.
//! - Personal/peripheral essence pools (page 2): `EPCheck1`–`EPCheck40`,
//!   `APCheck1`–`APCheck60`.
//!
//! Each enum-keyed table below (`attribute_dots`, `ability_dots`,
//! `virtue_dots`, `virtue_channels`, `ability_caste_mark`) is a `match`
//! against the corresponding `*Kind` enum — match exhaustiveness is what
//! enforces "every variant has a mapping", so adding a new variant is a
//! compile error here rather than a silent length mismatch.
//!
//! Re-run `cargo run --example dump_dots` to regenerate the underlying
//! coordinate dump if the template ever changes.

use crate::character::{AbilityKind, AttributeKind, VirtueKind};
use crate::rules::health::HealthLevelKind;

// ---------------------------------------------------------------------------
// Attributes — dot1..dot45
// ---------------------------------------------------------------------------

pub(super) const fn attribute_dots(kind: AttributeKind) -> [&'static str; 5] {
    match kind {
        AttributeKind::Strength => ["dot1", "dot2", "dot3", "dot4", "dot5"],
        AttributeKind::Dexterity => ["dot6", "dot7", "dot8", "dot9", "dot10"],
        AttributeKind::Stamina => ["dot11", "dot12", "dot13", "dot14", "dot15"],
        AttributeKind::Charisma => ["dot16", "dot17", "dot18", "dot19", "dot20"],
        AttributeKind::Manipulation => ["dot21", "dot22", "dot23", "dot24", "dot25"],
        AttributeKind::Appearance => ["dot26", "dot27", "dot28", "dot29", "dot30"],
        AttributeKind::Perception => ["dot31", "dot32", "dot33", "dot34", "dot35"],
        AttributeKind::Intelligence => ["dot36", "dot37", "dot38", "dot39", "dot40"],
        AttributeKind::Wits => ["dot41", "dot42", "dot43", "dot44", "dot45"],
    }
}

// ---------------------------------------------------------------------------
// Abilities — dot46..dot170 (first 25 of 30 slots; slots 26-30 stay blank)
// ---------------------------------------------------------------------------

pub(super) const fn ability_dots(kind: AbilityKind) -> [&'static str; 5] {
    match kind {
        AbilityKind::Archery => ["dot46", "dot47", "dot48", "dot49", "dot50"],
        AbilityKind::MartialArts => ["dot51", "dot52", "dot53", "dot54", "dot55"],
        AbilityKind::Melee => ["dot56", "dot57", "dot58", "dot59", "dot60"],
        AbilityKind::Thrown => ["dot61", "dot62", "dot63", "dot64", "dot65"],
        AbilityKind::War => ["dot66", "dot67", "dot68", "dot69", "dot70"],
        AbilityKind::Integrity => ["dot96", "dot97", "dot98", "dot99", "dot100"],
        AbilityKind::Performance => ["dot101", "dot102", "dot103", "dot104", "dot105"],
        AbilityKind::Presence => ["dot106", "dot107", "dot108", "dot109", "dot110"],
        AbilityKind::Resistance => ["dot111", "dot112", "dot113", "dot114", "dot115"],
        AbilityKind::Survival => ["dot116", "dot117", "dot118", "dot119", "dot120"],
        AbilityKind::Craft => ["dot146", "dot147", "dot148", "dot149", "dot150"],
        AbilityKind::Investigation => ["dot151", "dot152", "dot153", "dot154", "dot155"],
        AbilityKind::Lore => ["dot156", "dot157", "dot158", "dot159", "dot160"],
        AbilityKind::Medicine => ["dot161", "dot162", "dot163", "dot164", "dot165"],
        AbilityKind::Occult => ["dot166", "dot167", "dot168", "dot169", "dot170"],
        AbilityKind::Athletics => ["dot71", "dot72", "dot73", "dot74", "dot75"],
        AbilityKind::Awareness => ["dot76", "dot77", "dot78", "dot79", "dot80"],
        AbilityKind::Dodge => ["dot81", "dot82", "dot83", "dot84", "dot85"],
        AbilityKind::Larceny => ["dot86", "dot87", "dot88", "dot89", "dot90"],
        AbilityKind::Stealth => ["dot91", "dot92", "dot93", "dot94", "dot95"],
        AbilityKind::Bureaucracy => ["dot121", "dot122", "dot123", "dot124", "dot125"],
        AbilityKind::Linguistics => ["dot126", "dot127", "dot128", "dot129", "dot130"],
        AbilityKind::Ride => ["dot131", "dot132", "dot133", "dot134", "dot135"],
        AbilityKind::Sail => ["dot136", "dot137", "dot138", "dot139", "dot140"],
        AbilityKind::Socialize => ["dot141", "dot142", "dot143", "dot144", "dot145"],
    }
}

pub(super) const fn ability_caste_mark(kind: AbilityKind) -> &'static str {
    match kind {
        AbilityKind::Archery => "skillscheck1",
        AbilityKind::MartialArts => "skillscheck2",
        AbilityKind::Melee => "skillscheck3",
        AbilityKind::Thrown => "skillscheck4",
        AbilityKind::War => "skillscheck5",
        AbilityKind::Integrity => "skillscheck11",
        AbilityKind::Performance => "skillscheck12",
        AbilityKind::Presence => "skillscheck13",
        AbilityKind::Resistance => "skillscheck14",
        AbilityKind::Survival => "skillscheck15",
        AbilityKind::Craft => "skillscheck21",
        AbilityKind::Investigation => "skillscheck22",
        AbilityKind::Lore => "skillscheck23",
        AbilityKind::Medicine => "skillscheck24",
        AbilityKind::Occult => "skillscheck25",
        AbilityKind::Athletics => "skillscheck6",
        AbilityKind::Awareness => "skillscheck7",
        AbilityKind::Dodge => "skillscheck8",
        AbilityKind::Larceny => "skillscheck9",
        AbilityKind::Stealth => "skillscheck10",
        AbilityKind::Bureaucracy => "skillscheck16",
        AbilityKind::Linguistics => "skillscheck17",
        AbilityKind::Ride => "skillscheck18",
        AbilityKind::Sail => "skillscheck19",
        AbilityKind::Socialize => "skillscheck20",
    }
}

// ---------------------------------------------------------------------------
// Specialties — 15 write-in rows total: 5 on page 1 (column 3 of the ability
// grid) plus 10 on page 4. Page-1 rows have a caste/favored tick box; page-4
// rows don't. Page-1 rating dots are `dot171`–`dot195`; page-4 dots are
// `e2dot51`–`e2dot100` and text labels are `specx1`–`specx10`.
// ---------------------------------------------------------------------------

pub(super) const SPECIALTY_ROWS: usize = 15;
pub(super) const SPECIALTY_PAGE1_ROWS: usize = 5;

pub(super) const SPECIALTY_PAGE4_DOTS: [[&str; 5]; 10] = [
    ["e2dot51", "e2dot52", "e2dot53", "e2dot54", "e2dot55"],
    ["e2dot56", "e2dot57", "e2dot58", "e2dot59", "e2dot60"],
    ["e2dot61", "e2dot62", "e2dot63", "e2dot64", "e2dot65"],
    ["e2dot66", "e2dot67", "e2dot68", "e2dot69", "e2dot70"],
    ["e2dot71", "e2dot72", "e2dot73", "e2dot74", "e2dot75"],
    ["e2dot76", "e2dot77", "e2dot78", "e2dot79", "e2dot80"],
    ["e2dot81", "e2dot82", "e2dot83", "e2dot84", "e2dot85"],
    ["e2dot86", "e2dot87", "e2dot88", "e2dot89", "e2dot90"],
    ["e2dot91", "e2dot92", "e2dot93", "e2dot94", "e2dot95"],
    ["e2dot96", "e2dot97", "e2dot98", "e2dot99", "e2dot100"],
];

pub(super) fn specialty_dots(row: usize) -> Option<[&'static str; 5]> {
    match row {
        0 => Some(["dot171", "dot172", "dot173", "dot174", "dot175"]),
        1 => Some(["dot176", "dot177", "dot178", "dot179", "dot180"]),
        2 => Some(["dot181", "dot182", "dot183", "dot184", "dot185"]),
        3 => Some(["dot186", "dot187", "dot188", "dot189", "dot190"]),
        4 => Some(["dot191", "dot192", "dot193", "dot194", "dot195"]),
        5..=14 => Some(SPECIALTY_PAGE4_DOTS[row - SPECIALTY_PAGE1_ROWS]),
        _ => None,
    }
}

/// Page-1 specialty rows (0–4) have a caste/favored tick box; page-4 rows
/// (5–14) don't.
pub(super) const fn specialty_caste_mark(row: usize) -> Option<&'static str> {
    match row {
        0 => Some("skillscheck26"),
        1 => Some("skillscheck27"),
        2 => Some("skillscheck28"),
        3 => Some("skillscheck29"),
        4 => Some("skillscheck30"),
        _ => None,
    }
}

/// Text label field for a given specialty row. Page-1 rows use the
/// `specialties{N}` slots; page-4 rows use the separate `specx{N}` slots.
pub(super) fn specialty_text_field(row: usize) -> Option<String> {
    match row {
        0..=4 => Some(format!("specialties{}", row + 1)),
        5..=14 => Some(format!("specx{}", row - SPECIALTY_PAGE1_ROWS + 1)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Virtues — rating dots and channel diamonds, both keyed by VirtueKind
// ---------------------------------------------------------------------------

pub(super) const fn virtue_dots(kind: VirtueKind) -> [&'static str; 5] {
    match kind {
        VirtueKind::Compassion => ["xtdot1", "xtdot2", "xtdot3", "xtdot4", "xtdot5"],
        VirtueKind::Conviction => ["xtdot11", "xtdot12", "xtdot13", "xtdot14", "xtdot15"],
        VirtueKind::Temperance => ["xtdot6", "xtdot7", "xtdot8", "xtdot9", "xtdot10"],
        VirtueKind::Valor => ["xtdot16", "xtdot17", "xtdot18", "xtdot19", "xtdot20"],
    }
}

pub(super) const fn virtue_channels(kind: VirtueKind) -> [&'static str; 5] {
    match kind {
        VirtueKind::Compassion => [
            "virtuecheck1",
            "virtuecheck2",
            "virtuecheck3",
            "virtuecheck4",
            "virtuecheck5",
        ],
        VirtueKind::Conviction => [
            "virtuecheck11",
            "virtuecheck12",
            "virtuecheck13",
            "virtuecheck14",
            "virtuecheck15",
        ],
        VirtueKind::Temperance => [
            "virtuecheck6",
            "virtuecheck7",
            "virtuecheck8",
            "virtuecheck9",
            "virtuecheck10",
        ],
        VirtueKind::Valor => [
            "virtuecheck16",
            "virtuecheck17",
            "virtuecheck18",
            "virtuecheck19",
            "virtuecheck20",
        ],
    }
}

// ---------------------------------------------------------------------------
// Willpower / Essence / Limit
// ---------------------------------------------------------------------------

pub(super) const WILLPOWER_DOTS: [&str; 10] = [
    "willdot1",
    "willdot2",
    "willdot3",
    "willdot4",
    "willdot5",
    "willdot6",
    "willdot7",
    "willdot8",
    "willdot9",
    "willdot10",
];

/// Willpower temporary track (boxes you check off when spending willpower).
pub(super) const WILLPOWER_TEMP: [&str; 10] = [
    "willcheck1",
    "willcheck2",
    "willcheck3",
    "willcheck4",
    "willcheck5",
    "willcheck6",
    "willcheck7",
    "willcheck8",
    "willcheck9",
    "willcheck10",
];

/// Essence dots — the MrGone sheet only carries six AcroForm dot fields,
/// even though the rules scale runs 1–10 (Exalted 2E core, p.78–79).
/// Positions 7–10 are drawn as overlay content by `dots.rs`, not via
/// AcroForm fields.
pub(super) const ESSENCE_DOTS: [&str; 6] = [
    "essencedot1",
    "essencedot2",
    "essencedot3",
    "essencedot4",
    "essencedot5",
    "essencedot6",
];

/// Limit track (10 boxes).
pub(super) const LIMIT_TRACK: [&str; 10] = [
    "LBCheck1",
    "LBCheck2",
    "LBCheck3",
    "LBCheck4",
    "LBCheck5",
    "LBCheck6",
    "LBCheck7",
    "LBCheck8",
    "LBCheck9",
    "LBCheck10",
];

/// Per-language checkboxes (15 rows). Ticked iff that slot's language
/// is the character's native tongue.
pub(super) const LANGUAGE_CHECKBOXES: [&str; 15] = [
    "LCheck1", "LCheck2", "LCheck3", "LCheck4", "LCheck5", "LCheck6", "LCheck7", "LCheck8",
    "LCheck9", "LCheck10", "LCheck11", "LCheck12", "LCheck13", "LCheck14", "LCheck15",
];

// ---------------------------------------------------------------------------
// Health (22 boxes, grouped by wound penalty / kind)
//
// The MrGone sheet lays out the health track in semantic rows: each row
// holds the *default* level(s) at a given penalty plus reserved slots for
// Ox-Body Technique extras. `-2` is split visually across two PDF rows
// (5+5) but is one logical bucket. There are no PDF slots for Dying rows.
// ---------------------------------------------------------------------------

pub(super) const HEALTH_SLOTS_ZERO: [&str; 5] = [
    "healthcheck1",
    "healthcheck2",
    "healthcheck3",
    "healthcheck4",
    "healthcheck5",
];

pub(super) const HEALTH_SLOTS_MINUS_ONE: [&str; 5] = [
    "healthcheck6",
    "healthcheck7",
    "healthcheck8",
    "healthcheck9",
    "healthcheck10",
];

pub(super) const HEALTH_SLOTS_MINUS_TWO: [&str; 10] = [
    "healthcheck11",
    "healthcheck12",
    "healthcheck13",
    "healthcheck14",
    "healthcheck15",
    "healthcheck16",
    "healthcheck17",
    "healthcheck18",
    "healthcheck19",
    "healthcheck20",
];

pub(super) const HEALTH_SLOTS_MINUS_FOUR: [&str; 1] = ["healthcheck21"];

pub(super) const HEALTH_SLOTS_INCAP: [&str; 1] = ["healthcheck22"];

/// Every PDF health checkbox in template order. Used by the "fields exist"
/// test and by the fill code's clear-unused pass.
pub(super) const ALL_HEALTH_SLOTS: [&str; 22] = [
    "healthcheck1",
    "healthcheck2",
    "healthcheck3",
    "healthcheck4",
    "healthcheck5",
    "healthcheck6",
    "healthcheck7",
    "healthcheck8",
    "healthcheck9",
    "healthcheck10",
    "healthcheck11",
    "healthcheck12",
    "healthcheck13",
    "healthcheck14",
    "healthcheck15",
    "healthcheck16",
    "healthcheck17",
    "healthcheck18",
    "healthcheck19",
    "healthcheck20",
    "healthcheck21",
    "healthcheck22",
];

/// Slots available for a given `(penalty, kind)` bucket. Returns an empty
/// slice for buckets the PDF can't render (Dying rows).
pub(super) fn health_slots(penalty: i8, kind: HealthLevelKind) -> &'static [&'static str] {
    match (kind, penalty) {
        (HealthLevelKind::Wound, 0) => &HEALTH_SLOTS_ZERO,
        (HealthLevelKind::Wound, -1) => &HEALTH_SLOTS_MINUS_ONE,
        (HealthLevelKind::Wound, -2) => &HEALTH_SLOTS_MINUS_TWO,
        (HealthLevelKind::Wound, -4) => &HEALTH_SLOTS_MINUS_FOUR,
        (HealthLevelKind::Wound, _) => &[],
        (HealthLevelKind::Incap, _) => &HEALTH_SLOTS_INCAP,
        (HealthLevelKind::Dying, _) => &[],
    }
}

// ---------------------------------------------------------------------------
// Backgrounds — 18 slots total: 8 on page 1 (dot241..dot280) and 10 on page 4
// (e2dot1..e2dot50). Page-4 text labels use `backgrounds9`..`backgrounds18`.
//
// Backgrounds are chosen at character creation, so slots have no fixed
// semantic identity — they stay indexed by position.
// ---------------------------------------------------------------------------

pub(super) const BACKGROUND_PAGE1_SLOTS: usize = 8;
pub(super) const BACKGROUND_PAGE4_SLOTS: usize = 10;
pub(super) const BACKGROUND_SLOTS: usize = BACKGROUND_PAGE1_SLOTS + BACKGROUND_PAGE4_SLOTS;

pub(super) const BACKGROUND_SLOT_DOTS: [[&str; 5]; 8] = [
    ["dot241", "dot242", "dot243", "dot244", "dot245"],
    ["dot246", "dot247", "dot248", "dot249", "dot250"],
    ["dot251", "dot252", "dot253", "dot254", "dot255"],
    ["dot256", "dot257", "dot258", "dot259", "dot260"],
    ["dot261", "dot262", "dot263", "dot264", "dot265"],
    ["dot266", "dot267", "dot268", "dot269", "dot270"],
    ["dot271", "dot272", "dot273", "dot274", "dot275"],
    ["dot276", "dot277", "dot278", "dot279", "dot280"],
];

pub(super) const BACKGROUND_PAGE4_DOTS: [[&str; 5]; 10] = [
    ["e2dot1", "e2dot2", "e2dot3", "e2dot4", "e2dot5"],
    ["e2dot6", "e2dot7", "e2dot8", "e2dot9", "e2dot10"],
    ["e2dot11", "e2dot12", "e2dot13", "e2dot14", "e2dot15"],
    ["e2dot16", "e2dot17", "e2dot18", "e2dot19", "e2dot20"],
    ["e2dot21", "e2dot22", "e2dot23", "e2dot24", "e2dot25"],
    ["e2dot26", "e2dot27", "e2dot28", "e2dot29", "e2dot30"],
    ["e2dot31", "e2dot32", "e2dot33", "e2dot34", "e2dot35"],
    ["e2dot36", "e2dot37", "e2dot38", "e2dot39", "e2dot40"],
    ["e2dot41", "e2dot42", "e2dot43", "e2dot44", "e2dot45"],
    ["e2dot46", "e2dot47", "e2dot48", "e2dot49", "e2dot50"],
];

/// Dot row for a given background slot (0..18). `None` past slot 17.
pub(super) fn background_dots(slot: usize) -> Option<[&'static str; 5]> {
    match slot {
        0..=7 => Some(BACKGROUND_SLOT_DOTS[slot]),
        8..=17 => Some(BACKGROUND_PAGE4_DOTS[slot - BACKGROUND_PAGE1_SLOTS]),
        _ => None,
    }
}

/// Text label field for a given background slot. Slots 0–7 use
/// `backgrounds1`..`backgrounds8` on page 1; slots 8–17 use
/// `backgrounds9`..`backgrounds18` on page 4.
pub(super) fn background_text_field(slot: usize) -> Option<String> {
    if slot < BACKGROUND_SLOTS {
        Some(format!("backgrounds{}", slot + 1))
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Intimacies — 10 slots × 10 rating dots, on page 4. Text labels reuse the
// existing `intimacies1`..`intimacies10` fields (also page 4 in this template).
// ---------------------------------------------------------------------------

pub(super) const INTIMACY_DOTS: [[&str; 10]; 10] = [
    [
        "Idot1", "Idot2", "Idot3", "Idot4", "Idot5", "Idot6", "Idot7", "Idot8", "Idot9", "Idot10",
    ],
    [
        "Idot11", "Idot12", "Idot13", "Idot14", "Idot15", "Idot16", "Idot17", "Idot18", "Idot19",
        "Idot20",
    ],
    [
        "Idot21", "Idot22", "Idot23", "Idot24", "Idot25", "Idot26", "Idot27", "Idot28", "Idot29",
        "Idot30",
    ],
    [
        "Idot31", "Idot32", "Idot33", "Idot34", "Idot35", "Idot36", "Idot37", "Idot38", "Idot39",
        "Idot40",
    ],
    [
        "Idot41", "Idot42", "Idot43", "Idot44", "Idot45", "Idot46", "Idot47", "Idot48", "Idot49",
        "Idot50",
    ],
    [
        "Idot51", "Idot52", "Idot53", "Idot54", "Idot55", "Idot56", "Idot57", "Idot58", "Idot59",
        "Idot60",
    ],
    [
        "Idot61", "Idot62", "Idot63", "Idot64", "Idot65", "Idot66", "Idot67", "Idot68", "Idot69",
        "Idot70",
    ],
    [
        "Idot71", "Idot72", "Idot73", "Idot74", "Idot75", "Idot76", "Idot77", "Idot78", "Idot79",
        "Idot80",
    ],
    [
        "Idot81", "Idot82", "Idot83", "Idot84", "Idot85", "Idot86", "Idot87", "Idot88", "Idot89",
        "Idot90",
    ],
    [
        "Idot91", "Idot92", "Idot93", "Idot94", "Idot95", "Idot96", "Idot97", "Idot98", "Idot99",
        "Idot100",
    ],
];

// ---------------------------------------------------------------------------
// Familiar — page 4 has a 30-checkbox health track plus a `fam1` text field
// for the familiar's name. Other `fam*` fields exist on the sheet but aren't
// modeled yet.
// ---------------------------------------------------------------------------

pub(super) const FAMILIAR_HEALTH_SLOTS: [&str; 30] = [
    "Fcheck1", "Fcheck2", "Fcheck3", "Fcheck4", "Fcheck5", "Fcheck6", "Fcheck7", "Fcheck8",
    "Fcheck9", "Fcheck10", "Fcheck11", "Fcheck12", "Fcheck13", "Fcheck14", "Fcheck15", "Fcheck16",
    "Fcheck17", "Fcheck18", "Fcheck19", "Fcheck20", "Fcheck21", "Fcheck22", "Fcheck23", "Fcheck24",
    "Fcheck25", "Fcheck26", "Fcheck27", "Fcheck28", "Fcheck29", "Fcheck30",
];

pub(super) const FAMILIAR_NAME_FIELD: &str = "fam1";

// ---------------------------------------------------------------------------
// Essence pools (page 2)
// ---------------------------------------------------------------------------

/// Personal essence mote-tracking checkboxes (40 boxes).
pub(super) const PERSONAL_ESSENCE_BOXES: usize = 40;

/// Peripheral essence mote-tracking checkboxes (60 boxes).
pub(super) const PERIPHERAL_ESSENCE_BOXES: usize = 60;

pub(super) fn personal_essence_field(i: usize) -> String {
    format!("EPCheck{}", i + 1)
}

pub(super) fn peripheral_essence_field(i: usize) -> String {
    format!("APCheck{}", i + 1)
}

// ---------------------------------------------------------------------------
// Dot accessors — call sites use these to fetch a specific dot index for a
// given trait kind, without caring about the row layout above.
// ---------------------------------------------------------------------------

pub(super) fn attribute_dot(kind: AttributeKind, dot_idx: usize) -> Option<&'static str> {
    attribute_dots(kind).get(dot_idx).copied()
}

pub(super) fn ability_dot(kind: AbilityKind, dot_idx: usize) -> Option<&'static str> {
    ability_dots(kind).get(dot_idx).copied()
}

pub(super) fn virtue_dot(kind: VirtueKind, dot_idx: usize) -> Option<&'static str> {
    virtue_dots(kind).get(dot_idx).copied()
}

pub(super) fn virtue_channel(kind: VirtueKind, channel_idx: usize) -> Option<&'static str> {
    virtue_channels(kind).get(channel_idx).copied()
}

// ---------------------------------------------------------------------------
// Sanity asserts
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn dot_fields_are_unique() {
        let mut seen: HashSet<String> = HashSet::new();
        let mut check = |f: &str| {
            assert!(seen.insert(f.to_string()), "duplicate dot field {}", f);
        };
        for kind in AttributeKind::ALL {
            for f in attribute_dots(*kind) {
                check(f);
            }
        }
        for kind in AbilityKind::ALL {
            for f in ability_dots(*kind) {
                check(f);
            }
            check(ability_caste_mark(*kind));
        }
        for row in 0..SPECIALTY_ROWS {
            for f in specialty_dots(row).expect("specialty row in range") {
                check(f);
            }
            // Only page-1 specialty rows have caste/favored marks.
            if let Some(field) = specialty_caste_mark(row) {
                check(field);
            }
        }
        for slot in 0..BACKGROUND_SLOTS {
            for f in background_dots(slot).expect("background slot in range") {
                check(f);
            }
        }
        for row in &INTIMACY_DOTS {
            for f in row {
                check(f);
            }
        }
        for f in &FAMILIAR_HEALTH_SLOTS {
            check(f);
        }
        for kind in VirtueKind::ALL {
            for f in virtue_dots(*kind) {
                check(f);
            }
            for f in virtue_channels(*kind) {
                check(f);
            }
        }
        for f in &WILLPOWER_DOTS {
            check(f);
        }
        for f in &WILLPOWER_TEMP {
            check(f);
        }
        for f in &ESSENCE_DOTS {
            check(f);
        }
        for f in &LIMIT_TRACK {
            check(f);
        }
        for f in &LANGUAGE_CHECKBOXES {
            check(f);
        }
        for f in &ALL_HEALTH_SLOTS {
            check(f);
        }
    }

    #[test]
    fn health_buckets_partition_all_health_slots() {
        let mut bucketed: Vec<&'static str> = Vec::new();
        bucketed.extend_from_slice(&HEALTH_SLOTS_ZERO);
        bucketed.extend_from_slice(&HEALTH_SLOTS_MINUS_ONE);
        bucketed.extend_from_slice(&HEALTH_SLOTS_MINUS_TWO);
        bucketed.extend_from_slice(&HEALTH_SLOTS_MINUS_FOUR);
        bucketed.extend_from_slice(&HEALTH_SLOTS_INCAP);

        let bucketed_set: HashSet<&'static str> = bucketed.iter().copied().collect();
        let all_set: HashSet<&'static str> = ALL_HEALTH_SLOTS.iter().copied().collect();

        assert_eq!(
            bucketed.len(),
            bucketed_set.len(),
            "duplicate field across health buckets"
        );
        assert_eq!(
            bucketed_set, all_set,
            "buckets do not cover ALL_HEALTH_SLOTS"
        );
    }

    #[test]
    fn referenced_fields_exist_in_template() {
        let doc = super::super::template::load_template().expect("load template");
        let index = super::super::acroform::FieldIndex::build(&doc);
        let mut missing: Vec<String> = Vec::new();
        let mut probe = |f: &str| {
            if !index.has(f) {
                missing.push(f.to_string());
            }
        };
        for kind in AttributeKind::ALL {
            for f in attribute_dots(*kind) {
                probe(f);
            }
        }
        for kind in AbilityKind::ALL {
            for f in ability_dots(*kind) {
                probe(f);
            }
            probe(ability_caste_mark(*kind));
        }
        for row in 0..SPECIALTY_ROWS {
            for f in specialty_dots(row).expect("specialty row in range") {
                probe(f);
            }
            if let Some(field) = specialty_caste_mark(row) {
                probe(field);
            }
            if let Some(field) = specialty_text_field(row) {
                probe(&field);
            }
        }
        for slot in 0..BACKGROUND_SLOTS {
            for f in background_dots(slot).expect("background slot in range") {
                probe(f);
            }
            if let Some(field) = background_text_field(slot) {
                probe(&field);
            }
        }
        for row in &INTIMACY_DOTS {
            for f in row {
                probe(f);
            }
        }
        for f in &FAMILIAR_HEALTH_SLOTS {
            probe(f);
        }
        probe(FAMILIAR_NAME_FIELD);
        for kind in VirtueKind::ALL {
            for f in virtue_dots(*kind) {
                probe(f);
            }
            for f in virtue_channels(*kind) {
                probe(f);
            }
        }
        for f in WILLPOWER_DOTS
            .iter()
            .chain(WILLPOWER_TEMP.iter())
            .chain(ESSENCE_DOTS.iter())
            .chain(LIMIT_TRACK.iter())
            .chain(LANGUAGE_CHECKBOXES.iter())
            .chain(ALL_HEALTH_SLOTS.iter())
        {
            probe(f);
        }
        for i in 0..PERSONAL_ESSENCE_BOXES {
            let f = personal_essence_field(i);
            if !index.has(&f) {
                missing.push(f);
            }
        }
        for i in 0..PERIPHERAL_ESSENCE_BOXES {
            let f = peripheral_essence_field(i);
            if !index.has(&f) {
                missing.push(f);
            }
        }
        assert!(missing.is_empty(), "missing fields: {:?}", missing);
    }
}
