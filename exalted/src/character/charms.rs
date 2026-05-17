use serde::{Deserialize, Serialize};

use super::traits::{AbilityKind, DotSource};
use crate::rules::health::OxBodyPattern;

fn default_ability() -> AbilityKind {
    AbilityKind::Lore
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChosenCharm {
    pub name: String,
    pub source: DotSource,
    #[serde(default = "default_ability")]
    pub ability: AbilityKind,
    #[serde(default)]
    pub non_solar: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ox_body_pattern: Option<OxBodyPattern>,
}

impl ChosenCharm {
    pub fn new(name: impl Into<String>, ability: AbilityKind, source: DotSource) -> Self {
        Self {
            name: name.into(),
            source,
            ability,
            non_solar: false,
            notes: None,
            ox_body_pattern: None,
        }
    }
}
