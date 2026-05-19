pub mod backgrounds;
pub mod charms;
pub mod equipment;
pub mod hearthstone;
pub mod identity;
pub mod intimacies;
pub mod languages;
pub mod spells;
pub mod state;
pub mod traits;
pub mod xp;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub use backgrounds::{BackgroundInstance, BackgroundKind};
pub use charms::CharmRef;
pub use equipment::{Armor, Artifact, Equipment, Possession, Weapon};
pub use hearthstone::Hearthstone;
pub use identity::{Anima, Appearance, Caste, Identity, VirtueFlaw};
pub use intimacies::{Intimacy, IntimacyKind};
pub use languages::{KnownLanguage, LanguageFamily};
pub use spells::{SpellCircle, SpellRef};
pub use state::{HealthDamage, MoteCommitment, MotePool, PoolState};
pub use traits::{
    AbilityKind, AttributeGroup, AttributeKind, AttributePriority, DotPurchase, DotSource,
    RatedTrait, Specialty, VirtueKind,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Character {
    pub identity: Identity,
    pub caste: Caste,
    pub favored_abilities: Vec<AbilityKind>,
    pub attributes: BTreeMap<AttributeKind, RatedTrait>,
    pub attribute_priority: AttributePriority,
    pub abilities: BTreeMap<AbilityKind, RatedTrait>,
    pub virtues: BTreeMap<VirtueKind, RatedTrait>,
    pub primary_virtue: Option<VirtueKind>,
    pub virtue_flaw: Option<VirtueFlaw>,
    pub willpower: RatedTrait,
    pub essence: RatedTrait,
    pub charms: Vec<CharmRef>,
    #[serde(default)]
    pub spells: Vec<SpellRef>,
    #[serde(default)]
    pub backgrounds: Vec<BackgroundInstance>,
    pub intimacies: Vec<Intimacy>,
    pub equipment: Equipment,
    pub xp_earned: u32,
    pub xp_banked: u32,
    pub pool_state: PoolState,
    pub notes: BTreeMap<String, String>,
    #[serde(default)]
    pub languages: Vec<KnownLanguage>,
    #[serde(default)]
    pub hearthstones: Vec<Hearthstone>,
}

impl Character {
    /// Build a blank Solar character with default starting traits (1 dot per
    /// Attribute, 0 dots per Ability, 1 dot per Virtue, Essence 2). All
    /// purchase logs are empty — the character is not yet a "valid" sheet
    /// until chargen allocations are made.
    pub fn new_blank_solar(name: impl Into<String>, caste: Caste) -> Self {
        let mut attributes = BTreeMap::new();
        for a in AttributeKind::ALL {
            attributes.insert(*a, RatedTrait::with_base(1));
        }
        let mut abilities = BTreeMap::new();
        for a in AbilityKind::ALL {
            abilities.insert(*a, RatedTrait::with_base(0));
        }
        let mut virtues = BTreeMap::new();
        for v in VirtueKind::ALL {
            virtues.insert(*v, RatedTrait::with_base(1));
        }
        Self {
            identity: Identity {
                name: name.into(),
                ..Identity::default()
            },
            caste,
            favored_abilities: Vec::new(),
            attributes,
            attribute_priority: AttributePriority::default(),
            abilities,
            virtues,
            primary_virtue: None,
            virtue_flaw: None,
            willpower: RatedTrait::with_base(0),
            essence: RatedTrait::with_base(2),
            charms: Vec::new(),
            spells: Vec::new(),
            backgrounds: Vec::new(),
            intimacies: Vec::new(),
            equipment: Equipment::default(),
            xp_earned: 0,
            xp_banked: 0,
            pool_state: PoolState::default(),
            notes: BTreeMap::new(),
            languages: Vec::new(),
            hearthstones: Vec::new(),
        }
    }

    pub fn attribute(&self, kind: AttributeKind) -> u8 {
        self.attributes
            .get(&kind)
            .map(|t| t.dots())
            .unwrap_or(0)
    }

    pub fn ability(&self, kind: AbilityKind) -> u8 {
        self.abilities.get(&kind).map(|t| t.dots()).unwrap_or(0)
    }

    pub fn virtue(&self, kind: VirtueKind) -> u8 {
        self.virtues.get(&kind).map(|t| t.dots()).unwrap_or(0)
    }

    /// Sum of dots across every `BackgroundInstance` of the given kind.
    pub fn background(&self, kind: BackgroundKind) -> u8 {
        self.backgrounds
            .iter()
            .filter(|b| b.kind == kind)
            .map(|b| b.trait_.dots())
            .sum()
    }

    pub fn backgrounds_of(
        &self,
        kind: BackgroundKind,
    ) -> impl Iterator<Item = &BackgroundInstance> {
        self.backgrounds.iter().filter(move |b| b.kind == kind)
    }

    pub fn essence_dots(&self) -> u8 {
        self.essence.dots()
    }

    pub fn willpower_dots(&self) -> u8 {
        self.willpower.dots()
    }

    pub fn is_caste_ability(&self, ability: AbilityKind) -> bool {
        self.caste.caste_abilities().contains(&ability)
    }

    pub fn is_favored_ability(&self, ability: AbilityKind) -> bool {
        self.favored_abilities.contains(&ability)
    }

    pub fn is_caste_or_favored_ability(&self, ability: AbilityKind) -> bool {
        self.is_caste_ability(ability) || self.is_favored_ability(ability)
    }

    pub fn validate_chargen(&self) -> crate::error::ValidationReport {
        crate::rules::chargen::validate_chargen(self)
    }

    pub fn validate_xp(&self) -> crate::error::ValidationReport {
        crate::rules::chargen::validate_xp(self)
    }
}
