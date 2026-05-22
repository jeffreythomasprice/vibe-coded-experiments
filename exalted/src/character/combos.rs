use serde::{Deserialize, Serialize};

use super::traits::DotSource;

/// A named Combo on the character sheet — a pre-purchased bundle of Charms
/// the Exalt can activate together as a single action (p.244–247).
///
/// `charm_ids` are the ids of the member Charms; each must match the id of
/// a `CharmRef` already on the character (`CharmRef::id()`). The order is
/// preserved as written so the rendered listing stays stable.
///
/// `source` follows the same convention as `CharmRef::source()`:
///   - `BonusPoints { spent }` for Combos bought at chargen
///     (cost = number of member Charms).
///   - `Xp { spent }` for Combos bought in play
///     (cost = sum of member Charms' minimum Ability ratings).
///
/// `ChargenPriority` is not a valid source — Combos do not consume a Charm
/// slot. Validation in `check_combos` rejects it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Combo {
    pub name: String,
    pub charm_ids: Vec<String>,
    pub source: DotSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}
