//! Chargen point economy and validation. Source: `rules/character_creation.md`,
//! which summarizes Exalted 2E pp. 76–85.
//!
//! BP costs encoded here:
//! - Attribute: 4 BP per dot.
//! - Ability: 1 BP per dot (Caste/Favored) or 2 BP per dot (other).
//! - Specialty: 1 BP per specialty (other), 1 BP per 2 specialties (C/F).
//! - Background: 1 BP per dot (≤3), 2 BP per dot (4–5).
//! - Charm: 4 BP (Caste/Favored), 5 BP (other).
//! - Combo: BP equal to the number of Charms it contains (p.246).
//! - Virtue: 3 BP per dot.
//! - Willpower: 2 BP per dot.
//! - Essence: 7 BP per dot.
//! - Intimacy beyond Compassion baseline: 3 BP each.

use crate::character::{
    AbilityKind, AttributeGroup, BackgroundKind, Caste, Character, SpellCircle, VirtueKind,
};
use crate::error::{ValidationError, ValidationReport};
use crate::rules::database::prereq::{self, Prereq};
use crate::rules::defense::willpower_from_virtues;
use crate::rules::xp_costs::NON_SOLAR_CHARM_XP_COST;

pub const TOTAL_BONUS_POINTS: u32 = 15;
pub const ABILITY_CHARGEN_POOL: u32 = 28;
pub const VIRTUE_CHARGEN_POOL: u32 = 5;
pub const BACKGROUND_CHARGEN_POOL: u32 = 7;
pub const TOTAL_CHARMS_AT_CHARGEN: usize = 10;
pub const MIN_CASTE_FAVORED_ABILITY_DOTS: u32 = 10;
pub const MIN_CASTE_FAVORED_CHARMS: usize = 5;
pub const ESSENCE_MAX_AT_CHARGEN: u8 = 5;

pub fn attribute_dot_bp_cost(_new_rating: u8) -> u32 {
    4
}

pub fn ability_dot_bp_cost(_new_rating: u8, caste_or_favored: bool) -> u32 {
    if caste_or_favored { 1 } else { 2 }
}

/// Specialty BP cost: 1 per specialty out of caste/favored. Within a
/// caste/favored ability, two specialties cost 1 BP — callers must aggregate
/// per-ability counts and use `specialty_bp_cost_for_ability` for the total.
pub fn specialty_bp_cost(caste_or_favored: bool) -> f32 {
    if caste_or_favored { 0.5 } else { 1.0 }
}

/// Aggregate BP cost for `n_cf` caste/favored specialties + `n_oc` out-of-caste
/// specialties on a single ability: `ceil(n_cf / 2) + n_oc`.
pub fn specialty_bp_cost_for_ability(n_cf: usize, n_oc: usize) -> u32 {
    let cf_cost = (n_cf as u32 + 1) / 2;
    cf_cost + n_oc as u32
}

pub fn background_dot_bp_cost(new_rating: u8) -> u32 {
    if new_rating <= 3 { 1 } else { 2 }
}

pub fn charm_bp_cost(caste_or_favored: bool) -> u32 {
    if caste_or_favored { 4 } else { 5 }
}

/// BP cost of a Combo at chargen: one BP per member Charm (p.246).
pub fn combo_bp_cost(num_charms: usize) -> u32 {
    num_charms as u32
}

pub fn virtue_dot_bp_cost(_new_rating: u8) -> u32 {
    3
}

pub fn willpower_dot_bp_cost(_new_rating: u8) -> u32 {
    2
}

pub fn essence_dot_bp_cost(_new_rating: u8) -> u32 {
    7
}

pub fn intimacy_bp_cost() -> u32 {
    3
}

/// Run every chargen rule against the character. Collects all failures into
/// a single `ValidationReport`.
pub fn validate_chargen(character: &Character) -> ValidationReport {
    let mut report = ValidationReport::new();

    check_favored(character, &mut report);
    check_attribute_priority(character, &mut report);
    check_abilities(character, &mut report);
    check_virtues(character, &mut report);
    check_backgrounds(character, &mut report);
    check_charms(character, &mut report);
    check_combos(character, &mut report);
    check_intimacies(character, &mut report);
    check_essence_at_chargen(character, &mut report);
    check_willpower_base(character, &mut report);
    check_willpower_bp_cap(character, &mut report);
    check_bonus_point_total(character, &mut report);
    validate_bp(character, &mut report);
    crate::rules::languages::validate_languages(character, &mut report);
    crate::rules::backgrounds::validate_backgrounds(character, &mut report);
    report.extend(crate::rules::equipment::validate_hearthstones(character));
    report.extend(crate::rules::equipment::validate_artifacts(character));

    report
}

fn check_favored(c: &Character, report: &mut ValidationReport) {
    if c.favored_abilities.len() != 5 {
        report.push(ValidationError::FavoredAbilityCount {
            got: c.favored_abilities.len(),
        });
    }

    // distinct check
    let mut seen = std::collections::BTreeSet::new();
    for a in &c.favored_abilities {
        seen.insert(*a);
    }
    if seen.len() != c.favored_abilities.len() {
        report.push(ValidationError::FavoredAbilityCount {
            got: c.favored_abilities.len(),
        });
    }

    let caste = c.caste.caste_abilities();
    for ability in c.favored_abilities.iter() {
        if caste.contains(ability) {
            report.push(ValidationError::CasteFavoredOverlap {
                ability: format!("{ability:?}"),
            });
        }
    }
}

fn check_attribute_priority(c: &Character, report: &mut ValidationReport) {
    if !c.attribute_priority.is_well_formed() {
        report.push(ValidationError::AttributePriorityMisassigned);
        return;
    }

    for group in AttributeGroup::ALL {
        let group_total: u32 = group
            .members()
            .iter()
            .map(|attr| {
                c.attributes
                    .get(attr)
                    .map(|t| t.chargen_priority_dots() as u32)
                    .unwrap_or(0)
            })
            .sum();
        let expected = c.attribute_priority.dots_for(*group);
        if group_total != expected {
            report.push(ValidationError::AttributePriorityDotsWrong {
                group: format!("{group:?}"),
                got: group_total,
                expected,
            });
        }
    }

    // No attribute > 5 (base + all sources).
    for (attr, trait_) in &c.attributes {
        if trait_.dots() > 5 {
            report.push(ValidationError::AttributeOverMax {
                attribute: format!("{attr:?}"),
                got: trait_.dots(),
            });
        }
    }
}

fn check_abilities(c: &Character, report: &mut ValidationReport) {
    let chargen_total: u32 = c
        .abilities
        .values()
        .map(|t| t.chargen_priority_dots() as u32)
        .sum();
    if chargen_total != ABILITY_CHARGEN_POOL {
        report.push(ValidationError::AbilityChargenDotsWrong {
            got: chargen_total,
        });
    }

    let cf_total: u32 = c
        .abilities
        .iter()
        .filter(|(a, _)| c.is_caste_or_favored_ability(**a))
        .map(|(_, t)| t.chargen_priority_dots() as u32)
        .sum();
    if cf_total < MIN_CASTE_FAVORED_ABILITY_DOTS {
        report.push(ValidationError::CasteFavoredDotsTooLow { got: cf_total });
    }

    // Each Favored Ability needs at least 1 chargen-priority dot.
    for fav in &c.favored_abilities {
        let dots = c
            .abilities
            .get(fav)
            .map(|t| t.chargen_priority_dots())
            .unwrap_or(0);
        if dots == 0 {
            report.push(ValidationError::FavoredAbilityZeroDots {
                ability: format!("{fav:?}"),
            });
        }
    }

    // No ability above 3 from chargen priority alone.
    for (a, t) in &c.abilities {
        if t.chargen_priority_dots() > 3 {
            report.push(ValidationError::AbilityChargenOverThree {
                ability: format!("{a:?}"),
                got: t.chargen_priority_dots(),
            });
        }
    }

    // Specialty caps: rules p.77 — max 3 distinct specialties per Ability,
    // and max 3 dots within any one specialty. Linguistics is exempt on both
    // (each dialect is its own specialty, with no dot cap on the dialect
    // dice bonus — see `rules::dice::dice_pool`).
    for (a, t) in &c.abilities {
        if *a == AbilityKind::Linguistics {
            continue;
        }
        let aggregated = t.aggregated_specialties();
        if aggregated.len() > 3 {
            report.push(ValidationError::SpecialtiesOverMax {
                ability: format!("{a:?}"),
                got: aggregated.len(),
            });
        }
        for (name, dots) in aggregated {
            if dots > 3 {
                report.push(ValidationError::SpecialtyOverMaxDots {
                    ability: format!("{a:?}"),
                    specialty: name,
                    got: dots,
                });
            }
        }
    }
}

fn check_virtues(c: &Character, report: &mut ValidationReport) {
    let chargen_total: u32 = c
        .virtues
        .values()
        .map(|t| t.chargen_priority_dots() as u32)
        .sum();
    if chargen_total != VIRTUE_CHARGEN_POOL {
        report.push(ValidationError::VirtueChargenDotsWrong {
            got: chargen_total,
        });
    }

    match c.primary_virtue {
        Some(primary) => {
            let primary_dots = c.virtue(primary);
            if primary_dots < 3 {
                report.push(ValidationError::PrimaryVirtueTooLow {
                    virtue: format!("{primary:?}"),
                    got: primary_dots,
                });
            }
        }
        None => report.push(ValidationError::PrimaryVirtueUnset),
    }

    match (&c.virtue_flaw, c.primary_virtue) {
        (None, _) => report.push(ValidationError::VirtueFlawUnset),
        (Some(flaw), Some(primary)) => {
            let expected = flaw.flaw_virtue();
            if expected != primary {
                report.push(ValidationError::VirtueFlawMismatch {
                    flaw: format!("{flaw:?}"),
                    expected_virtue: format!("{expected:?}"),
                    got_virtue: format!("{primary:?}"),
                });
            }
        }
        (Some(_), None) => {}
    }

    // No virtue > 4 from chargen priority + base alone.
    for (v, t) in &c.virtues {
        let from_chargen_and_base = t.base_dots + t.chargen_priority_dots();
        if from_chargen_and_base > 4 {
            report.push(ValidationError::VirtueChargenOverFour {
                virtue: format!("{v:?}"),
                got: from_chargen_and_base,
            });
        }
    }
}

fn check_backgrounds(c: &Character, report: &mut ValidationReport) {
    let db = crate::rules::database::database();

    // Flag any Lookup ref whose id isn't in the database. This catches typos
    // and stale references early — the entry can't be resolved so subsequent
    // kind-based checks would silently treat it as not-of-any-kind.
    for bg in &c.backgrounds {
        if let crate::character::BackgroundRef::Lookup { id, .. } = bg {
            if db.background(id).is_none() {
                report.push(ValidationError::UnknownBackgroundId { id: id.clone() });
            }
        }
    }

    let chargen_total: u32 = c
        .backgrounds
        .iter()
        .map(|b| b.trait_().chargen_priority_dots() as u32)
        .sum();
    if chargen_total != BACKGROUND_CHARGEN_POOL {
        report.push(ValidationError::BackgroundChargenDotsWrong {
            got: chargen_total,
        });
    }

    for bg in &c.backgrounds {
        if bg.trait_().chargen_priority_dots() > 3 {
            let name = bg.display_name(db);
            let label = if bg.label().is_empty() {
                name.to_string()
            } else {
                format!("{}({})", name, bg.label())
            };
            report.push(ValidationError::BackgroundChargenOverThree {
                background: label,
                got: bg.trait_().chargen_priority_dots(),
            });
        }
    }

    let cult_creation_dots: u8 = c
        .backgrounds_of(BackgroundKind::Cult)
        .map(|b| b.trait_().chargen_priority_dots() + b.trait_().bonus_point_dots())
        .sum();
    if cult_creation_dots > 2 {
        report.push(ValidationError::CultOverTwoAtCreation {
            got: cult_creation_dots,
        });
    }
}

fn check_charms(c: &Character, report: &mut ValidationReport) {
    // 10 "starting" picks come out of the chargen pool. Spells acquired via
    // the sorcery swap (`character_creation.md` §6.2) compete for those same
    // 10 slots — each `ChargenPriority` spell replaces one Charm. BP-bought
    // charms/spells are optional extras and don't count toward this total.
    let chargen_charms = c
        .charms
        .iter()
        .filter(|ch| ch.source().is_chargen_priority())
        .count();
    let chargen_spells = c
        .spells
        .iter()
        .filter(|sp| sp.source().is_chargen_priority())
        .count();
    let chargen_picks = chargen_charms + chargen_spells;
    if chargen_picks != TOTAL_CHARMS_AT_CHARGEN {
        report.push(ValidationError::CharmCountWrong { got: chargen_picks });
    }

    // p.77: Solar Circle spells are forbidden at character creation —
    // neither via the sorcery swap nor BP.
    let db = crate::rules::database::database();
    for spell in &c.spells {
        let src = spell.source();
        if spell.circle(db) == Some(SpellCircle::Solar)
            && (src.is_chargen_priority() || src.is_bonus_points())
        {
            report.push(ValidationError::SolarCircleAtChargen {
                spell: spell.display_name(db).to_string(),
            });
        }
    }

    // Every known spell requires the matching Sorcery initiation charm.
    // Necromancy circles aren't in the database yet, so only the three
    // Sorcery circles are gated here.
    for spell in &c.spells {
        let required = match spell.circle(db) {
            Some(SpellCircle::Terrestrial) => Some("terrestrial-circle-sorcery"),
            Some(SpellCircle::Celestial) => Some("celestial-circle-sorcery"),
            Some(SpellCircle::Solar) => Some("solar-circle-sorcery"),
            _ => None,
        };
        if let Some(req) = required
            && !c.charms.iter().any(|ch| ch.is_id(req))
        {
            report.push(ValidationError::SpellRequiresSorceryCharm {
                spell: spell.display_name(db).to_string(),
                charm: req.to_string(),
            });
        }
    }


    let mut cf_chargen_count = 0usize;
    for charm in &c.charms {
        let display = charm.display_name(db).to_string();
        if charm.non_solar() && c.caste != Caste::Eclipse {
            report.push(ValidationError::AlienCharmOnNonEclipse {
                charm: display.clone(),
            });
        }
        match charm.entry(db) {
            Some(def) => {
                // Some catalog entries (Dragon-Blooded Anima Powers etc.) have
                // no specific ability. Ability/min-ability checks only apply
                // when the entry resolves to a concrete AbilityKind.
                if let Some(ability) = def.ability_kind() {
                    let ability_dots = c.ability(ability);
                    if ability_dots < def.mins_ability {
                        report.push(ValidationError::CharmAbilityBelowMin {
                            charm: display.clone(),
                            ability: format!("{ability:?}"),
                            required: def.mins_ability,
                            got: ability_dots,
                        });
                    }
                    if charm.source().is_chargen_priority()
                        && c.is_caste_or_favored_ability(ability)
                    {
                        cf_chargen_count += 1;
                    }
                }
                if c.essence_dots() < def.mins_essence {
                    report.push(ValidationError::CharmEssenceBelowMin {
                        charm: display.clone(),
                        required: def.mins_essence,
                        got: c.essence_dots(),
                    });
                }
                for (attr, &required) in &def.mins_attribute {
                    let got = c.attribute(*attr);
                    if got < required {
                        report.push(ValidationError::CharmAttributeBelowMin {
                            charm: display.clone(),
                            attribute: format!("{attr:?}"),
                            required,
                            got,
                        });
                    }
                }
                // Resolve each prereq string into a structured `Prereq` and
                // hard-error if unsatisfied. Plain charm ids look up the
                // character's charm list; `any-X-excellency` and
                // `any-N-X-excellencies` patterns match against the five
                // derived Excellencies for that ability.
                for raw in &def.prerequisites {
                    let p = prereq::parse_prereq(raw);
                    if prereq::satisfied_by(&p, c, db) {
                        continue;
                    }
                    match p {
                        Prereq::Charm(id) => {
                            report.push(ValidationError::CharmPrereqMissing {
                                charm: display.clone(),
                                missing: id,
                            });
                        }
                        Prereq::AnyExcellencies { ability, count } if count <= 1 => {
                            report.push(
                                ValidationError::CharmPrereqAnyExcellencyMissing {
                                    charm: display.clone(),
                                    ability: crate::rules::database::ability_display_name(
                                        ability,
                                    )
                                    .to_string(),
                                },
                            );
                        }
                        Prereq::AnyExcellencies { ability, count } => {
                            let ids = prereq::excellency_ids_for(ability);
                            let got = ids
                                .iter()
                                .filter(|id| c.charms.iter().any(|ch| ch.is_id(id)))
                                .count() as u8;
                            report.push(
                                ValidationError::CharmPrereqNExcellenciesMissing {
                                    charm: display.clone(),
                                    ability: crate::rules::database::ability_display_name(
                                        ability,
                                    )
                                    .to_string(),
                                    required: count,
                                    got,
                                },
                            );
                        }
                    }
                }
            }
            None => {
                report.push_note(ValidationError::UnknownCharm { charm: display });
            }
        }
    }

    if cf_chargen_count < MIN_CASTE_FAVORED_CHARMS {
        report.push(ValidationError::CasteFavoredCharmsTooFew {
            got: cf_chargen_count,
        });
    }

    // p.209 / character_creation.md §5.5: Ox-Body Technique stacks up to the
    // character's Resistance rating. Extra picks were previously silently
    // dropped by health::ox_body_patterns; flag them so the player notices
    // that BP/XP and a Charm slot are being wasted.
    let resistance = c.ability(AbilityKind::Resistance);
    let ox_body_picks = c
        .charms
        .iter()
        .filter(|ch| ch.is_id("ox-body-technique"))
        .count();
    if ox_body_picks > resistance as usize {
        report.push(ValidationError::OxBodyOverResistance {
            got: ox_body_picks,
            max: resistance,
        });
    }
}

fn check_combos(c: &Character, report: &mut ValidationReport) {
    let db = crate::rules::database::database();
    use crate::rules::database::CharmType;

    for combo in &c.combos {
        let combo_name = if combo.name.is_empty() {
            "<unnamed>".to_string()
        } else {
            combo.name.clone()
        };

        match combo.source {
            crate::character::DotSource::BonusPoints { .. }
            | crate::character::DotSource::Xp { .. } => {}
            other => {
                report.push(ValidationError::ComboInvalidSource {
                    combo: combo_name.clone(),
                    source_kind: format!("{:?}", other),
                });
            }
        }

        let mut seen: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
        for id in &combo.charm_ids {
            if !seen.insert(id.as_str()) {
                report.push(ValidationError::ComboDuplicateCharm {
                    combo: combo_name.clone(),
                    charm: id.clone(),
                });
            }
        }

        let mut member_entries = Vec::new();
        let mut has_basic = false;
        let mut simple_count = 0usize;

        for id in &combo.charm_ids {
            let owned = c.charms.iter().find(|ch| ch.is_id(id));
            let Some(charm_ref) = owned else {
                report.push(ValidationError::ComboCharmNotOwned {
                    combo: combo_name.clone(),
                    charm: id.clone(),
                });
                continue;
            };
            let Some(entry) = charm_ref.entry(db) else {
                report.push_note(ValidationError::UnknownCharm {
                    charm: charm_ref.display_name(db).to_string(),
                });
                continue;
            };
            let comboable = entry
                .keywords
                .iter()
                .any(|k| k.eq_ignore_ascii_case("Combo-OK") || k.eq_ignore_ascii_case("Combo-Basic"));
            if !comboable {
                report.push(ValidationError::ComboCharmNotComboable {
                    combo: combo_name.clone(),
                    charm: entry.name.clone(),
                });
            }
            let is_basic = entry
                .keywords
                .iter()
                .any(|k| k.eq_ignore_ascii_case("Combo-Basic"));
            if is_basic {
                has_basic = true;
            }
            if matches!(entry.charm_type, CharmType::Simple) {
                simple_count += 1;
            }
            member_entries.push((entry, is_basic));
        }

        if has_basic {
            for (entry, is_basic) in &member_entries {
                if *is_basic {
                    continue;
                }
                if !matches!(entry.charm_type, CharmType::Reflexive) {
                    report.push(ValidationError::ComboBasicWithNonReflexive {
                        combo: combo_name.clone(),
                        charm: entry.name.clone(),
                    });
                }
            }
        }

        if simple_count > 1 {
            report.push(ValidationError::ComboMultipleSimple {
                combo: combo_name.clone(),
                count: simple_count,
            });
        }
    }
}

fn check_intimacies(c: &Character, report: &mut ValidationReport) {
    let compassion = c.virtue(VirtueKind::Compassion);
    let willpower = c.willpower_dots();
    let max = willpower.saturating_add(compassion);
    let got = c.intimacies.len();
    if got > max as usize {
        report.push(ValidationError::IntimaciesOverMax { got, max });
    }
    if got < compassion as usize {
        report.push(ValidationError::IntimaciesBelowCompassion {
            got,
            min: compassion,
        });
    }
}

fn check_essence_at_chargen(c: &Character, report: &mut ValidationReport) {
    if c.essence_dots() > ESSENCE_MAX_AT_CHARGEN {
        report.push(ValidationError::EssenceOverMaxAtChargen {
            got: c.essence_dots(),
        });
    }
}

fn check_willpower_base(c: &Character, report: &mut ValidationReport) {
    let expected = willpower_from_virtues(c);
    let base = c.willpower.base_dots;
    if base != expected {
        report.push(ValidationError::WillpowerNotFromVirtues { expected, got: base });
    }
    if c.willpower_dots() > 10 {
        report.push(ValidationError::WillpowerOverTen {
            got: c.willpower_dots(),
        });
    }
}

fn check_willpower_bp_cap(c: &Character, report: &mut ValidationReport) {
    let virtues_at_four_plus = VirtueKind::ALL
        .iter()
        .filter(|v| c.virtue(**v) >= 4)
        .count();
    let bp_dots = c.willpower.bonus_point_dots();
    if bp_dots > 0 {
        let wp = c.willpower_dots();
        if wp > 8 && virtues_at_four_plus < 2 {
            report.push(ValidationError::WillpowerOverEightWithoutHighVirtues { got: wp });
        }
    }
}

fn check_bonus_point_total(c: &Character, report: &mut ValidationReport) {
    let total = crate::character::xp::total_bp_spent(c);
    if total != TOTAL_BONUS_POINTS {
        report.push(ValidationError::BonusPointsWrong { got: total });
    }
}

/// Reconstruct the trait rating sequence and recompute the canonical XP cost
/// of each `Xp` purchase. Returns the total expected XP spent on that trait
/// + the list of any mismatches.
pub fn recompute_xp_for_increases(
    starting_rating: u8,
    purchases: &[crate::character::DotPurchase],
    cost_for_rating: impl Fn(u8) -> u32,
) -> (u32, Vec<(u32, u32)>) {
    let mut rating = starting_rating;
    let mut total = 0u32;
    let mut mismatches = Vec::new();
    for p in purchases {
        rating += 1;
        let canonical = cost_for_rating(rating);
        if let crate::character::DotSource::Xp { spent } = p.source {
            total += spent;
            if spent != canonical {
                mismatches.push((spent, canonical));
            }
        }
    }
    (total, mismatches)
}

/// Validate the BP ledger: every `BonusPoints { spent }` purchase costs the
/// canonical amount for its trait. Mirrors `validate_xp` but for chargen
/// bonus points. Called from `validate_chargen_with`.
///
/// Note: this does **not** check that the total BP spent equals
/// [`TOTAL_BONUS_POINTS`] — `check_bonus_point_total` already does that.
pub fn validate_bp(c: &Character, report: &mut ValidationReport) {
    // Attributes
    for (attr, t) in &c.attributes {
        let mut rating = t.base_dots;
        for p in &t.purchases {
            rating += 1;
            if let crate::character::DotSource::BonusPoints { spent } = p.source {
                let expected = attribute_dot_bp_cost(rating);
                if spent as u32 != expected {
                    report.push(ValidationError::BpCostWrong {
                        trait_name: format!("Attribute::{attr:?}"),
                        paid: spent as u32,
                        expected,
                    });
                }
            }
        }
    }

    // Abilities (dots)
    for (ab, t) in &c.abilities {
        let mut rating = t.base_dots;
        let cf = c.is_caste_or_favored_ability(*ab);
        for p in &t.purchases {
            rating += 1;
            if let crate::character::DotSource::BonusPoints { spent } = p.source {
                let expected = ability_dot_bp_cost(rating, cf);
                if spent as u32 != expected {
                    report.push(ValidationError::BpCostWrong {
                        trait_name: format!("Ability::{ab:?}"),
                        paid: spent as u32,
                        expected,
                    });
                }
            }
        }
    }

    // Specialties: aggregate per ability. C/F abilities cost 1 BP per 2
    // specialties (so two BP-spec entries at 1+0 BP each is correct); out-of
    // C/F abilities cost 1 BP per specialty.
    for (ab, t) in &c.abilities {
        let cf = c.is_caste_or_favored_ability(*ab);
        let mut bp_specs_n = 0usize;
        let mut bp_specs_spent = 0u32;
        for s in &t.specialties {
            if let crate::character::DotSource::BonusPoints { spent } = s.source {
                bp_specs_n += 1;
                bp_specs_spent += spent as u32;
            }
        }
        if bp_specs_n == 0 {
            continue;
        }
        let expected = if cf {
            specialty_bp_cost_for_ability(bp_specs_n, 0)
        } else {
            specialty_bp_cost_for_ability(0, bp_specs_n)
        };
        if bp_specs_spent != expected {
            report.push(ValidationError::BpCostWrong {
                trait_name: format!("Specialties::{ab:?} ({} entries)", bp_specs_n),
                paid: bp_specs_spent,
                expected,
            });
        }
    }

    // Backgrounds
    let db = crate::rules::database::database();
    for bg in &c.backgrounds {
        let t = bg.trait_();
        let mut rating = t.base_dots;
        for p in &t.purchases {
            rating += 1;
            if let crate::character::DotSource::BonusPoints { spent } = p.source {
                let expected = background_dot_bp_cost(rating);
                if spent as u32 != expected {
                    let name = bg.display_name(db);
                    let label = if bg.label().is_empty() {
                        format!("Background::{}", name)
                    } else {
                        format!("Background::{}({})", name, bg.label())
                    };
                    report.push(ValidationError::BpCostWrong {
                        trait_name: label,
                        paid: spent as u32,
                        expected,
                    });
                }
            }
        }
    }

    // Virtues
    for (v, t) in &c.virtues {
        let mut rating = t.base_dots;
        for p in &t.purchases {
            rating += 1;
            if let crate::character::DotSource::BonusPoints { spent } = p.source {
                let expected = virtue_dot_bp_cost(rating);
                if spent as u32 != expected {
                    report.push(ValidationError::BpCostWrong {
                        trait_name: format!("Virtue::{v:?}"),
                        paid: spent as u32,
                        expected,
                    });
                }
            }
        }
    }

    // Willpower
    {
        let mut rating = c.willpower.base_dots;
        for p in &c.willpower.purchases {
            rating += 1;
            if let crate::character::DotSource::BonusPoints { spent } = p.source {
                let expected = willpower_dot_bp_cost(rating);
                if spent as u32 != expected {
                    report.push(ValidationError::BpCostWrong {
                        trait_name: "Willpower".to_string(),
                        paid: spent as u32,
                        expected,
                    });
                }
            }
        }
    }

    // Essence
    {
        let mut rating = c.essence.base_dots;
        for p in &c.essence.purchases {
            rating += 1;
            if let crate::character::DotSource::BonusPoints { spent } = p.source {
                let expected = essence_dot_bp_cost(rating);
                if spent as u32 != expected {
                    report.push(ValidationError::BpCostWrong {
                        trait_name: "Essence".to_string(),
                        paid: spent as u32,
                        expected,
                    });
                }
            }
        }
    }

    // Charms
    let db = crate::rules::database::database();
    for charm in &c.charms {
        if let crate::character::DotSource::BonusPoints { spent } = charm.source() {
            // Caste/favored discount applies only when the charm resolves to
            // a specific ability — otherwise (Dragon-Blooded Anima Powers,
            // etc.) treat as out-of-caste-favored.
            let cf = charm
                .ability(db)
                .map(|a| c.is_caste_or_favored_ability(a))
                .unwrap_or(false);
            let expected = charm_bp_cost(cf);
            if spent as u32 != expected {
                report.push(ValidationError::BpCostWrong {
                    trait_name: format!("Charm::{}", charm.display_name(db)),
                    paid: spent as u32,
                    expected,
                });
            }
        }
    }

    // Spells (same cost as Charms; Occult discount).
    let occult_cf = c.is_caste_or_favored_ability(AbilityKind::Occult);
    for spell in &c.spells {
        if let crate::character::DotSource::BonusPoints { spent } = spell.source() {
            let expected = charm_bp_cost(occult_cf);
            if spent as u32 != expected {
                report.push(ValidationError::BpCostWrong {
                    trait_name: format!("Spell::{}", spell.display_name(db)),
                    paid: spent as u32,
                    expected,
                });
            }
        }
    }

    // Intimacies
    for intimacy in &c.intimacies {
        if let crate::character::DotSource::BonusPoints { spent } = intimacy.source {
            let expected = intimacy_bp_cost();
            if spent as u32 != expected {
                report.push(ValidationError::BpCostWrong {
                    trait_name: format!("Intimacy::{}", intimacy.description),
                    paid: spent as u32,
                    expected,
                });
            }
        }
    }

    // Combos (BP-bought at chargen: 1 BP per member Charm).
    for combo in &c.combos {
        if let crate::character::DotSource::BonusPoints { spent } = combo.source {
            let expected = combo_bp_cost(combo.charm_ids.len());
            if spent as u32 != expected {
                report.push(ValidationError::BpCostWrong {
                    trait_name: format!("Combo::{}", combo.name),
                    paid: spent as u32,
                    expected,
                });
            }
        }
    }
}

/// Validate the XP ledger: every `Xp { spent }` purchase costs the canonical
/// amount, total spent ≤ earned, banked = earned − spent.
pub fn validate_xp(c: &Character) -> ValidationReport {
    use crate::rules::xp_costs;

    let mut report = ValidationReport::new();

    // Attributes
    for (attr, t) in &c.attributes {
        let mut rating = t.base_dots;
        for p in &t.purchases {
            rating += 1;
            if let crate::character::DotSource::Xp { spent } = p.source {
                let canonical = xp_costs::xp_cost_attribute_increase(rating);
                if spent != canonical {
                    report.push(ValidationError::XpCostWrong {
                        trait_name: format!("Attribute::{attr:?}"),
                        paid: spent,
                        expected: canonical,
                    });
                }
            }
        }
    }

    // Abilities
    for (ab, t) in &c.abilities {
        let mut rating = t.base_dots;
        let favored_or_caste = c.is_caste_or_favored_ability(*ab);
        for p in &t.purchases {
            rating += 1;
            if let crate::character::DotSource::Xp { spent } = p.source {
                let canonical = if rating == 1 {
                    xp_costs::xp_cost_new_ability()
                } else {
                    xp_costs::xp_cost_ability_increase(rating, favored_or_caste)
                };
                if spent != canonical {
                    report.push(ValidationError::XpCostWrong {
                        trait_name: format!("Ability::{ab:?}"),
                        paid: spent,
                        expected: canonical,
                    });
                }
            }
        }
    }

    // Specialties (each = flat 3 XP)
    for (ab, t) in &c.abilities {
        for s in &t.specialties {
            if let crate::character::DotSource::Xp { spent } = s.source {
                let canonical = xp_costs::xp_cost_specialty();
                if spent != canonical {
                    report.push(ValidationError::XpCostWrong {
                        trait_name: format!("Specialty::{ab:?}::{}", s.name),
                        paid: spent,
                        expected: canonical,
                    });
                }
            }
        }
    }

    // Virtues
    for (v, t) in &c.virtues {
        let mut rating = t.base_dots;
        for p in &t.purchases {
            rating += 1;
            if let crate::character::DotSource::Xp { spent } = p.source {
                let canonical = xp_costs::xp_cost_virtue_increase(rating);
                if spent != canonical {
                    report.push(ValidationError::XpCostWrong {
                        trait_name: format!("Virtue::{v:?}"),
                        paid: spent,
                        expected: canonical,
                    });
                }
            }
        }
    }

    // Willpower
    {
        let mut rating = c.willpower.base_dots;
        for p in &c.willpower.purchases {
            rating += 1;
            if let crate::character::DotSource::Xp { spent } = p.source {
                let canonical = xp_costs::xp_cost_willpower_increase(rating);
                if spent != canonical {
                    report.push(ValidationError::XpCostWrong {
                        trait_name: "Willpower".to_string(),
                        paid: spent,
                        expected: canonical,
                    });
                }
            }
        }
    }

    // Essence
    {
        let mut rating = c.essence.base_dots;
        for p in &c.essence.purchases {
            rating += 1;
            if let crate::character::DotSource::Xp { spent } = p.source {
                let canonical = xp_costs::xp_cost_essence_increase(rating);
                if spent != canonical {
                    report.push(ValidationError::XpCostWrong {
                        trait_name: "Essence".to_string(),
                        paid: spent,
                        expected: canonical,
                    });
                }
            }
        }
    }

    // Charms (post-chargen XP-bought)
    let db = crate::rules::database::database();
    for charm in &c.charms {
        if let crate::character::DotSource::Xp { spent } = charm.source() {
            let canonical = if charm.non_solar() {
                NON_SOLAR_CHARM_XP_COST
            } else {
                let cf = charm
                    .ability(db)
                    .map(|a| c.is_caste_or_favored_ability(a))
                    .unwrap_or(false);
                xp_costs::xp_cost_charm(cf)
            };
            if spent != canonical {
                report.push(ValidationError::XpCostWrong {
                    trait_name: format!("Charm::{}", charm.display_name(db)),
                    paid: spent,
                    expected: canonical,
                });
            }
        }
    }

    // Spells (post-chargen XP-bought). Occult discount applies; no Solar
    // Circle restriction post-chargen.
    let occult_cf = c.is_caste_or_favored_ability(AbilityKind::Occult);
    for spell in &c.spells {
        if let crate::character::DotSource::Xp { spent } = spell.source() {
            let canonical = xp_costs::xp_cost_spell(occult_cf);
            if spent != canonical {
                report.push(ValidationError::XpCostWrong {
                    trait_name: format!("Spell::{}", spell.display_name(db)),
                    paid: spent,
                    expected: canonical,
                });
            }
        }
    }

    // Combos (post-chargen XP-bought: sum of member min-ability ratings).
    for combo in &c.combos {
        if let crate::character::DotSource::Xp { spent } = combo.source {
            let sum: u32 = combo
                .charm_ids
                .iter()
                .filter_map(|id| c.charms.iter().find(|ch| ch.is_id(id)))
                .filter_map(|ch| ch.entry(db))
                .map(|e| e.mins_ability as u32)
                .sum();
            let canonical = xp_costs::xp_cost_combo(sum);
            if spent != canonical {
                report.push(ValidationError::XpCostWrong {
                    trait_name: format!("Combo::{}", combo.name),
                    paid: spent,
                    expected: canonical,
                });
            }
        }
    }

    // XP totals
    let spent = crate::character::xp::total_xp_spent(c);
    if spent > c.xp_earned {
        report.push(ValidationError::XpOverspent {
            spent,
            earned: c.xp_earned,
        });
    }

    let expected_banked = c.xp_earned as i64 - spent as i64;
    if c.xp_banked as i64 != expected_banked {
        report.push(ValidationError::XpBankedWrong {
            banked: c.xp_banked,
            earned: c.xp_earned,
            spent,
            expected: expected_banked,
        });
    }

    report
}
