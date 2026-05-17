use serde::{Deserialize, Serialize};

use super::traits::AbilityKind;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub enum Caste {
    Dawn,
    Zenith,
    Twilight,
    Night,
    Eclipse,
}

impl Caste {
    pub fn caste_abilities(self) -> &'static [AbilityKind] {
        match self {
            Caste::Dawn => &[
                AbilityKind::Archery,
                AbilityKind::MartialArts,
                AbilityKind::Melee,
                AbilityKind::Thrown,
                AbilityKind::War,
            ],
            Caste::Zenith => &[
                AbilityKind::Integrity,
                AbilityKind::Performance,
                AbilityKind::Presence,
                AbilityKind::Resistance,
                AbilityKind::Survival,
            ],
            Caste::Twilight => &[
                AbilityKind::Craft,
                AbilityKind::Investigation,
                AbilityKind::Lore,
                AbilityKind::Medicine,
                AbilityKind::Occult,
            ],
            Caste::Night => &[
                AbilityKind::Athletics,
                AbilityKind::Awareness,
                AbilityKind::Dodge,
                AbilityKind::Larceny,
                AbilityKind::Stealth,
            ],
            Caste::Eclipse => &[
                AbilityKind::Bureaucracy,
                AbilityKind::Linguistics,
                AbilityKind::Ride,
                AbilityKind::Sail,
                AbilityKind::Socialize,
            ],
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Anima {
    pub totem: String,
    pub iconic_image: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Appearance {
    pub hair: String,
    pub eyes: String,
    pub skin: String,
    pub distinguishing_features: String,
    pub homeland: String,
    pub sex: String,
    pub age: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    pub name: String,
    pub concept: String,
    pub motivation: String,
    #[serde(default)]
    pub anima: Anima,
    #[serde(default)]
    pub appearance: Appearance,
    #[serde(default)]
    pub player: String,
    #[serde(default)]
    pub chronicle: String,
}

/// The Virtue Flaw chosen at chargen, driving Limit Break.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VirtueFlaw {
    CompassionateMartyrdom,
    HeartOfFlame,
    DeliberateCruelty,
    BerserkAnger,
    Contempt,
    Cowardice,
    Custom,
}
