pub mod backgrounds;
pub mod charms;
pub mod combos;
pub mod equipment;
pub mod familiar;
pub mod hearthstone;
pub mod identity;
pub mod intimacies;
pub mod languages;
pub mod notes;
pub mod spells;
pub mod state;
pub mod traits;
pub mod xp;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub use backgrounds::{BackgroundKind, BackgroundRef, canonical_id as background_canonical_id};
pub use charms::CharmRef;
pub use combos::Combo;
pub use equipment::{Armor, Artifact, Equipment, Possession, Weapon};
pub use familiar::Familiar;
pub use hearthstone::Hearthstone;
pub use identity::{Anima, Appearance, Caste, Identity, VirtueFlaw};
pub use intimacies::{Intimacy, IntimacyKind};
pub use languages::{KnownLanguage, LanguageFamily};
pub use notes::Note;
pub use spells::{SpellCircle, SpellRef};
pub use state::{HealthDamage, MoteCommitment, MotePool, PoolState};
pub use traits::{
    AbilityKind, AttributeGroup, AttributeKind, AttributePriority, Craft, DotPurchase, DotSource,
    RatedTrait, Specialty, VirtueKind,
};
pub use xp::XpAward;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Character {
    pub identity: Identity,
    pub caste: Caste,
    pub favored_abilities: Vec<AbilityKind>,
    pub attributes: BTreeMap<AttributeKind, RatedTrait>,
    pub attribute_priority: AttributePriority,
    pub abilities: BTreeMap<AbilityKind, RatedTrait>,
    /// Craft abilities, kept out of `abilities` because Craft is a family of
    /// separately-rated abilities (Craft: Water, Craft: Fire, …) that share a
    /// single Caste/Favored slot. The first entry is the primary craft that
    /// renders on the sheet's single Craft row. See [`Craft`].
    #[serde(default)]
    pub crafts: Vec<Craft>,
    pub virtues: BTreeMap<VirtueKind, RatedTrait>,
    pub primary_virtue: Option<VirtueKind>,
    pub virtue_flaw: Option<VirtueFlaw>,
    pub willpower: RatedTrait,
    pub essence: RatedTrait,
    pub charms: Vec<CharmRef>,
    #[serde(default)]
    pub combos: Vec<Combo>,
    #[serde(default)]
    pub spells: Vec<SpellRef>,
    #[serde(default)]
    pub backgrounds: Vec<BackgroundRef>,
    pub intimacies: Vec<Intimacy>,
    pub equipment: Equipment,
    pub xp_earned: u32,
    pub xp_banked: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub xp_awards: Vec<XpAward>,
    pub pool_state: PoolState,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<Note>,
    #[serde(default)]
    pub languages: Vec<KnownLanguage>,
    #[serde(default)]
    pub hearthstones: Vec<Hearthstone>,
    #[serde(default)]
    pub familiar: Option<Familiar>,
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
            // Craft is never stored in the ability map — it lives in `crafts`.
            if *a == AbilityKind::Craft {
                continue;
            }
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
            crafts: Vec::new(),
            virtues,
            primary_virtue: None,
            virtue_flaw: None,
            willpower: RatedTrait::with_base(0),
            essence: RatedTrait::with_base(2),
            charms: Vec::new(),
            combos: Vec::new(),
            spells: Vec::new(),
            backgrounds: Vec::new(),
            intimacies: Vec::new(),
            equipment: Equipment::default(),
            xp_earned: 0,
            xp_banked: 0,
            xp_awards: Vec::new(),
            pool_state: PoolState::default(),
            notes: Vec::new(),
            languages: Vec::new(),
            hearthstones: Vec::new(),
            familiar: None,
        }
    }

    pub fn attribute(&self, kind: AttributeKind) -> u8 {
        self.attributes.get(&kind).map(|t| t.dots()).unwrap_or(0)
    }

    pub fn ability(&self, kind: AbilityKind) -> u8 {
        // Craft has no entry in the ability map; its "rating" for purposes of
        // charm prerequisites and dice pools is the best of the character's
        // crafts — a "Craft N" requirement is met by having *a* Craft at N.
        if kind == AbilityKind::Craft {
            return self.craft_rating_max();
        }
        self.abilities.get(&kind).map(|t| t.dots()).unwrap_or(0)
    }

    /// The highest rating across all the character's crafts (0 if none).
    pub fn craft_rating_max(&self) -> u8 {
        self.crafts
            .iter()
            .map(|c| c.rating.dots())
            .max()
            .unwrap_or(0)
    }

    /// The primary craft — the one that renders on the sheet's single Craft
    /// row. Conventionally the first craft the character took.
    pub fn primary_craft(&self) -> Option<&Craft> {
        self.crafts.first()
    }

    /// Migrate a legacy character that stored Craft as an entry in the ability
    /// map into the `crafts` collection. Older TOML (and the GUI/CLI before
    /// multi-craft support) put a single `[abilities.Craft]` entry on the
    /// sheet; move any such entry with real content into `crafts`. A blank
    /// Craft entry is simply dropped. Idempotent and a no-op once migrated.
    pub fn migrate_legacy_craft(&mut self) {
        if let Some(rating) = self.abilities.remove(&AbilityKind::Craft) {
            let has_content =
                rating.dots() > 0 || !rating.purchases.is_empty() || !rating.specialties.is_empty();
            if has_content && self.crafts.is_empty() {
                self.crafts.push(Craft {
                    focus: String::new(),
                    rating,
                });
            }
        }
    }

    pub fn virtue(&self, kind: VirtueKind) -> u8 {
        self.virtues.get(&kind).map(|t| t.dots()).unwrap_or(0)
    }

    /// Sum of dots across every `BackgroundRef` resolving to the given kind.
    /// Lookup refs whose id isn't in the database contribute zero; chargen
    /// validation flags them separately as `UnknownBackgroundId`.
    pub fn background(&self, kind: BackgroundKind) -> u8 {
        let db = crate::rules::database::database();
        self.backgrounds
            .iter()
            .filter(|b| b.kind(db) == Some(kind))
            .map(|b| b.trait_().dots())
            .sum()
    }

    pub fn backgrounds_of(&self, kind: BackgroundKind) -> impl Iterator<Item = &BackgroundRef> {
        let db = crate::rules::database::database();
        self.backgrounds
            .iter()
            .filter(move |b| b.kind(db) == Some(kind))
    }

    pub fn essence_dots(&self) -> u8 {
        self.essence.dots()
    }

    /// True while the character is still in character-creation — i.e. no XP
    /// has been earned yet. Used by the UI to pick the default source for
    /// newly purchased dots (ChargenPriority / BonusPoints vs Xp).
    pub fn is_in_chargen(&self) -> bool {
        self.xp_earned == 0
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
