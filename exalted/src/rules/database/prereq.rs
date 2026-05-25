//! Parses charm prerequisite strings from the rules database into a structured
//! form, and decides whether a `Character` satisfies them.
//!
//! The TOML keeps prerequisites as opaque strings (`"there-is-no-wind"`,
//! `"any-archery-excellency"`, `"any-two-lore-excellencies"`) so authors don't
//! have to learn a parallel schema. This module is the interpreter.

use crate::character::{AbilityKind, Character};
use crate::rules::database::{RulesDatabase, ability_kebab_name};

/// Structured form of a single prerequisite string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Prereq {
    /// A specific charm id (the common case).
    Charm(String),
    /// "any-<ability>-excellency"   → count = 1
    /// "any-<N>-<ability>-excellencies" → count = N
    ///
    /// Satisfied when the character has at least `count` distinct charms drawn
    /// from the five derived excellencies for `ability`:
    ///   first-<ability>-excellency, second-<ability>-excellency,
    ///   third-<ability>-excellency, infinite-<ability>-mastery,
    ///   <ability>-essence-flow.
    AnyExcellencies { ability: AbilityKind, count: u8 },
}

/// Parse a raw prerequisite string. Patterns the parser recognizes:
///   - `any-<ability>-excellency`
///   - `any-<one|two|three|four|five>-<ability>-excellencies`
/// Anything else is treated as a literal charm id.
pub fn parse_prereq(raw: &str) -> Prereq {
    if let Some(rest) = raw.strip_prefix("any-") {
        // any-<ability>-excellency  (count = 1)
        if let Some(ability_part) = rest.strip_suffix("-excellency")
            && let Some(ability) = ability_from_kebab(ability_part)
        {
            return Prereq::AnyExcellencies { ability, count: 1 };
        }
        // any-<N>-<ability>-excellencies
        if let Some(middle) = rest.strip_suffix("-excellencies") {
            // middle is "<N>-<ability>" with N as an English number word.
            if let Some((count_word, ability_part)) = middle.split_once('-')
                && let Some(count) = number_word_to_u8(count_word)
                && let Some(ability) = ability_from_kebab(ability_part)
            {
                return Prereq::AnyExcellencies { ability, count };
            }
        }
    }
    Prereq::Charm(raw.to_string())
}

/// Match a kebab ability slug ("martial-arts", "archery") against `AbilityKind`.
fn ability_from_kebab(slug: &str) -> Option<AbilityKind> {
    AbilityKind::ALL
        .iter()
        .copied()
        .find(|a| ability_kebab_name(*a) == slug)
}

fn number_word_to_u8(word: &str) -> Option<u8> {
    match word {
        "one" => Some(1),
        "two" => Some(2),
        "three" => Some(3),
        "four" => Some(4),
        "five" => Some(5),
        _ => None,
    }
}

/// The five derived excellency ids for a given ability. Used both by
/// `satisfied_by` and (eventually) by anything else that wants to enumerate
/// "the Excellencies".
pub fn excellency_ids_for(ability: AbilityKind) -> [String; 5] {
    let k = ability_kebab_name(ability);
    [
        format!("first-{k}-excellency"),
        format!("second-{k}-excellency"),
        format!("third-{k}-excellency"),
        format!("infinite-{k}-mastery"),
        format!("{k}-essence-flow"),
    ]
}

/// Which of the five universal excellencies a character has learned for a
/// given ability. Each field maps to one of the ids returned by
/// `excellency_ids_for` in the same order.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KnownExcellencies {
    pub first: bool,
    pub second: bool,
    pub third: bool,
    pub infinite_mastery: bool,
    pub essence_flow: bool,
}

impl KnownExcellencies {
    pub fn any(self) -> bool {
        self.first || self.second || self.third || self.infinite_mastery || self.essence_flow
    }
}

/// Which universal excellencies the character has learned for `ability`.
pub fn known_excellencies(character: &Character, ability: AbilityKind) -> KnownExcellencies {
    let ids = excellency_ids_for(ability);
    let has = |id: &str| character.charms.iter().any(|ch| ch.is_id(id));
    KnownExcellencies {
        first: has(&ids[0]),
        second: has(&ids[1]),
        third: has(&ids[2]),
        infinite_mastery: has(&ids[3]),
        essence_flow: has(&ids[4]),
    }
}

/// True iff the character's `charms` list satisfies this prereq.
pub fn satisfied_by(prereq: &Prereq, character: &Character, _db: &RulesDatabase) -> bool {
    match prereq {
        Prereq::Charm(id) => character.charms.iter().any(|ch| ch.is_id(id)),
        Prereq::AnyExcellencies { ability, count } => {
            let needed = (*count) as usize;
            let ids = excellency_ids_for(*ability);
            let have = ids
                .iter()
                .filter(|id| character.charms.iter().any(|ch| ch.is_id(id)))
                .count();
            have >= needed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_charm_id() {
        assert_eq!(
            parse_prereq("there-is-no-wind"),
            Prereq::Charm("there-is-no-wind".to_string()),
        );
    }

    #[test]
    fn parses_any_single_excellency() {
        assert_eq!(
            parse_prereq("any-archery-excellency"),
            Prereq::AnyExcellencies {
                ability: AbilityKind::Archery,
                count: 1
            },
        );
    }

    #[test]
    fn parses_any_martial_arts_excellency() {
        assert_eq!(
            parse_prereq("any-martial-arts-excellency"),
            Prereq::AnyExcellencies {
                ability: AbilityKind::MartialArts,
                count: 1
            },
        );
    }

    #[test]
    fn parses_any_two_lore_excellencies() {
        assert_eq!(
            parse_prereq("any-two-lore-excellencies"),
            Prereq::AnyExcellencies {
                ability: AbilityKind::Lore,
                count: 2
            },
        );
    }

    #[test]
    fn unknown_string_falls_back_to_charm_id() {
        // Pre-fix template literal — if it ever sneaks through, treat it as a
        // missing charm id rather than silently passing.
        assert_eq!(
            parse_prereq("any-ability-excellency"),
            Prereq::Charm("any-ability-excellency".to_string()),
        );
    }

    #[test]
    fn excellency_ids_cover_all_five_templates() {
        let ids = excellency_ids_for(AbilityKind::Archery);
        assert_eq!(
            ids,
            [
                "first-archery-excellency",
                "second-archery-excellency",
                "third-archery-excellency",
                "infinite-archery-mastery",
                "archery-essence-flow",
            ]
            .map(String::from)
        );
    }
}
