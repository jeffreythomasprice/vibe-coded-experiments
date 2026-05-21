use serde::{Deserialize, Serialize};

use super::state::HealthDamage;

/// A bound familiar — paired with the Familiar background. The MrGone sheet
/// (page 4) has a name field and a 30-checkbox health track for the familiar.
/// Stats beyond name + damage aren't modeled yet; the rest of the `fam*`
/// fields on the sheet stay blank for now.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Familiar {
    pub name: String,
    #[serde(default)]
    pub health_damage: HealthDamage,
}
