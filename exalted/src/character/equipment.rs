use serde::{Deserialize, Serialize};

use super::traits::AbilityKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DamageType {
    Bashing,
    Lethal,
    Aggravated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Weapon {
    pub name: String,
    pub ability: AbilityKind,
    pub speed: i8,
    pub accuracy: i8,
    pub damage: i8,
    pub damage_type: DamageType,
    pub defense: i8,
    pub rate: u8,
    #[serde(default)]
    pub range_yards: Option<u32>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub attunement_motes: Option<u8>,
    #[serde(default)]
    pub artifact_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Armor {
    pub name: String,
    pub soak_bashing: u8,
    pub soak_lethal: u8,
    pub soak_aggravated: u8,
    #[serde(default)]
    pub hardness_bashing: u8,
    #[serde(default)]
    pub hardness_lethal: u8,
    pub mobility_penalty: u8,
    pub fatigue: u8,
    #[serde(default)]
    pub attunement_motes: Option<u8>,
    #[serde(default)]
    pub artifact_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Possession {
    pub name: String,
    pub primary_location: String,
    #[serde(default)]
    pub secondary_location: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Equipment {
    #[serde(default)]
    pub weapons: Vec<Weapon>,
    #[serde(default)]
    pub armor: Option<Armor>,
    #[serde(default)]
    pub other: Vec<Possession>,
    #[serde(default)]
    pub artifacts: Vec<Artifact>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    pub name: String,
    pub rating: u8,
    #[serde(default)]
    pub attunement_motes: u8,
    #[serde(default)]
    pub hearthstone_sockets: u8,
    #[serde(default)]
    pub socketed_hearthstones: Vec<String>,
    #[serde(default)]
    pub description: String,
}
