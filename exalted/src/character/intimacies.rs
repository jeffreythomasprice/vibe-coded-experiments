use serde::{Deserialize, Serialize};

use super::traits::DotSource;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
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
}
