use serde::{Deserialize, Serialize};

use super::traits::DotSource;

/// Sorcery circle for a known spell (p.77, §6.2 of `character_creation.md`).
/// Solars may learn Terrestrial and Celestial spells at character creation
/// via the "swap a Charm for a spell" exchange, but **Solar Circle spells
/// are forbidden at creation** and must be learnt post-chargen with XP.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub enum SpellCircle {
    Terrestrial,
    Celestial,
    Solar,
}

impl SpellCircle {
    pub const ALL: &'static [SpellCircle] = &[
        SpellCircle::Terrestrial,
        SpellCircle::Celestial,
        SpellCircle::Solar,
    ];
}

/// A single sorcery spell known by the character. The `source` mirrors
/// `ChosenCharm.source`: a spell taken via the chargen sorcery swap occupies
/// one of the 10 Charm picks (`ChargenPriority`), a BP-purchased spell costs
/// the same as a Charm (`BonusPoints`), and a post-chargen spell uses XP at
/// the `xp_cost_spell` rate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Spell {
    pub name: String,
    pub circle: SpellCircle,
    pub source: DotSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

impl Spell {
    pub fn new(name: impl Into<String>, circle: SpellCircle, source: DotSource) -> Self {
        Self {
            name: name.into(),
            circle,
            source,
            notes: None,
        }
    }
}
