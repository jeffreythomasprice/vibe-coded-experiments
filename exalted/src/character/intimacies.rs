use serde::{Deserialize, Serialize};

use super::traits::DotSource;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum IntimacyKind {
    Person,
    Place,
    Cause,
    Ideal,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Intimacy {
    pub description: String,
    pub kind: IntimacyKind,
    /// How the intimacy was acquired. Compassion-baseline intimacies use
    /// `DotSource::Base`; bonus-point or XP intimacies use the corresponding
    /// variant.
    pub source: DotSource,
    /// Strength of the intimacy on a 1–10 scale (the MrGone sheet's
    /// `Idot1`…`Idot10` row). Defaults to 1 for legacy data that predates
    /// the rating field.
    #[serde(default = "default_intimacy_rating")]
    pub rating: u8,
}

fn default_intimacy_rating() -> u8 {
    1
}
