use serde::{Deserialize, Serialize};

/// Where a single dot of a rated trait came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum DotSource {
    /// Free starting dot the rules give for nothing (Attribute=1, Virtue=1).
    Base,
    /// Allocated from the chargen point pools (attribute primary/secondary/
    /// tertiary, the 28-dot ability pool, the 5-dot virtue pool, the 7-dot
    /// background pool, the 10 starting charms).
    ChargenPriority,
    /// Bought with bonus points during chargen.
    BonusPoints { spent: u8 },
    /// Bought with experience after chargen.
    Xp { spent: u32 },
}

impl DotSource {
    pub fn bp_spent(self) -> u32 {
        match self {
            DotSource::BonusPoints { spent } => spent as u32,
            _ => 0,
        }
    }

    pub fn xp_spent(self) -> u32 {
        match self {
            DotSource::Xp { spent } => spent,
            _ => 0,
        }
    }

    pub fn is_chargen_priority(self) -> bool {
        matches!(self, DotSource::ChargenPriority)
    }

    pub fn is_bonus_points(self) -> bool {
        matches!(self, DotSource::BonusPoints { .. })
    }

    pub fn is_xp(self) -> bool {
        matches!(self, DotSource::Xp { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DotPurchase {
    pub source: DotSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl DotPurchase {
    pub fn new(source: DotSource) -> Self {
        Self { source, note: None }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Specialty {
    pub name: String,
    pub source: DotSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RatedTrait {
    pub base_dots: u8,
    #[serde(default)]
    pub purchases: Vec<DotPurchase>,
    #[serde(default)]
    pub specialties: Vec<Specialty>,
}

impl RatedTrait {
    pub fn with_base(base_dots: u8) -> Self {
        Self {
            base_dots,
            purchases: Vec::new(),
            specialties: Vec::new(),
        }
    }

    pub fn dots(&self) -> u8 {
        self.base_dots + self.purchases.len() as u8
    }

    /// Number of dots whose source is `ChargenPriority` (i.e. came out of a
    /// chargen pool like the 28-dot ability budget).
    pub fn chargen_priority_dots(&self) -> u8 {
        self.purchases
            .iter()
            .filter(|p| p.source.is_chargen_priority())
            .count() as u8
    }

    pub fn bonus_point_dots(&self) -> u8 {
        self.purchases
            .iter()
            .filter(|p| p.source.is_bonus_points())
            .count() as u8
    }

    pub fn xp_dots(&self) -> u8 {
        self.purchases.iter().filter(|p| p.source.is_xp()).count() as u8
    }

    pub fn bp_spent_on_dots(&self) -> u32 {
        self.purchases.iter().map(|p| p.source.bp_spent()).sum()
    }

    pub fn xp_spent_on_dots(&self) -> u32 {
        self.purchases.iter().map(|p| p.source.xp_spent()).sum()
    }

    pub fn xp_spent_on_specialties(&self) -> u32 {
        self.specialties.iter().map(|s| s.source.xp_spent()).sum()
    }

    /// Push a single dot from a chargen pool. Convenience helper for tests
    /// and chargen builders.
    pub fn add_chargen(&mut self) -> &mut Self {
        self.purchases
            .push(DotPurchase::new(DotSource::ChargenPriority));
        self
    }

    pub fn add_bonus(&mut self, spent: u8) -> &mut Self {
        self.purchases
            .push(DotPurchase::new(DotSource::BonusPoints { spent }));
        self
    }

    pub fn add_xp(&mut self, spent: u32) -> &mut Self {
        self.purchases
            .push(DotPurchase::new(DotSource::Xp { spent }));
        self
    }
}

// ---------------------------------------------------------------------------
// Attributes
// ---------------------------------------------------------------------------

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub enum AttributeKind {
    Strength,
    Dexterity,
    Stamina,
    Charisma,
    Manipulation,
    Appearance,
    Perception,
    Intelligence,
    Wits,
}

impl AttributeKind {
    pub const ALL: &'static [AttributeKind] = &[
        AttributeKind::Strength,
        AttributeKind::Dexterity,
        AttributeKind::Stamina,
        AttributeKind::Charisma,
        AttributeKind::Manipulation,
        AttributeKind::Appearance,
        AttributeKind::Perception,
        AttributeKind::Intelligence,
        AttributeKind::Wits,
    ];

    pub fn group(self) -> AttributeGroup {
        match self {
            AttributeKind::Strength | AttributeKind::Dexterity | AttributeKind::Stamina => {
                AttributeGroup::Physical
            }
            AttributeKind::Charisma
            | AttributeKind::Manipulation
            | AttributeKind::Appearance => AttributeGroup::Social,
            AttributeKind::Perception
            | AttributeKind::Intelligence
            | AttributeKind::Wits => AttributeGroup::Mental,
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub enum AttributeGroup {
    Physical,
    Social,
    Mental,
}

impl AttributeGroup {
    pub const ALL: &'static [AttributeGroup] = &[
        AttributeGroup::Physical,
        AttributeGroup::Social,
        AttributeGroup::Mental,
    ];

    pub fn members(self) -> &'static [AttributeKind] {
        match self {
            AttributeGroup::Physical => &[
                AttributeKind::Strength,
                AttributeKind::Dexterity,
                AttributeKind::Stamina,
            ],
            AttributeGroup::Social => &[
                AttributeKind::Charisma,
                AttributeKind::Manipulation,
                AttributeKind::Appearance,
            ],
            AttributeGroup::Mental => &[
                AttributeKind::Perception,
                AttributeKind::Intelligence,
                AttributeKind::Wits,
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttributePriority {
    pub primary: AttributeGroup,
    pub secondary: AttributeGroup,
    pub tertiary: AttributeGroup,
}

impl Default for AttributePriority {
    fn default() -> Self {
        Self {
            primary: AttributeGroup::Physical,
            secondary: AttributeGroup::Social,
            tertiary: AttributeGroup::Mental,
        }
    }
}

impl AttributePriority {
    pub fn dots_for(self, group: AttributeGroup) -> u32 {
        if group == self.primary {
            8
        } else if group == self.secondary {
            6
        } else if group == self.tertiary {
            4
        } else {
            0
        }
    }

    pub fn is_well_formed(self) -> bool {
        let mut groups = [self.primary, self.secondary, self.tertiary];
        groups.sort();
        groups == [AttributeGroup::Physical, AttributeGroup::Social, AttributeGroup::Mental]
    }
}

// ---------------------------------------------------------------------------
// Abilities
// ---------------------------------------------------------------------------

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub enum AbilityKind {
    // Dawn
    Archery,
    MartialArts,
    Melee,
    Thrown,
    War,
    // Zenith
    Integrity,
    Performance,
    Presence,
    Resistance,
    Survival,
    // Twilight
    Craft,
    Investigation,
    Lore,
    Medicine,
    Occult,
    // Night
    Athletics,
    Awareness,
    Dodge,
    Larceny,
    Stealth,
    // Eclipse
    Bureaucracy,
    Linguistics,
    Ride,
    Sail,
    Socialize,
}

impl AbilityKind {
    pub const ALL: &'static [AbilityKind] = &[
        AbilityKind::Archery,
        AbilityKind::MartialArts,
        AbilityKind::Melee,
        AbilityKind::Thrown,
        AbilityKind::War,
        AbilityKind::Integrity,
        AbilityKind::Performance,
        AbilityKind::Presence,
        AbilityKind::Resistance,
        AbilityKind::Survival,
        AbilityKind::Craft,
        AbilityKind::Investigation,
        AbilityKind::Lore,
        AbilityKind::Medicine,
        AbilityKind::Occult,
        AbilityKind::Athletics,
        AbilityKind::Awareness,
        AbilityKind::Dodge,
        AbilityKind::Larceny,
        AbilityKind::Stealth,
        AbilityKind::Bureaucracy,
        AbilityKind::Linguistics,
        AbilityKind::Ride,
        AbilityKind::Sail,
        AbilityKind::Socialize,
    ];
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub enum AbilityGroup {
    Combat,
    Physical,
    Social,
    Mental,
    Other,
}

// ---------------------------------------------------------------------------
// Virtues
// ---------------------------------------------------------------------------

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub enum VirtueKind {
    Compassion,
    Conviction,
    Temperance,
    Valor,
}

impl VirtueKind {
    pub const ALL: &'static [VirtueKind] = &[
        VirtueKind::Compassion,
        VirtueKind::Conviction,
        VirtueKind::Temperance,
        VirtueKind::Valor,
    ];
}
