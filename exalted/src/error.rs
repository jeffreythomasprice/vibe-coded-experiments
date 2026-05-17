use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ValidationError {
    #[error("caste/favored abilities overlap: {ability}")]
    CasteFavoredOverlap { ability: String },

    #[error("favored abilities must be 5 distinct entries (got {got})")]
    FavoredAbilityCount { got: usize },

    #[error("attribute priority groups must be exactly Physical+Social+Mental, one each")]
    AttributePriorityMisassigned,

    #[error(
        "attribute group {group} chargen-priority dots = {got}, expected {expected}"
    )]
    AttributePriorityDotsWrong {
        group: String,
        got: u32,
        expected: u32,
    },

    #[error("attribute {attribute} > 5 (got {got})")]
    AttributeOverMax { attribute: String, got: u8 },

    #[error("ability chargen-priority dots total = {got}, expected 28")]
    AbilityChargenDotsWrong { got: u32 },

    #[error("caste+favored ability dots < 10 (got {got})")]
    CasteFavoredDotsTooLow { got: u32 },

    #[error("favored ability {ability} has 0 dots; needs >= 1")]
    FavoredAbilityZeroDots { ability: String },

    #[error("ability {ability} has > 3 dots from chargen priority alone (got {got})")]
    AbilityChargenOverThree { ability: String, got: u8 },

    #[error("virtue chargen-priority dots total = {got}, expected 5")]
    VirtueChargenDotsWrong { got: u32 },

    #[error("primary virtue {virtue} must be >= 3 dots (got {got})")]
    PrimaryVirtueTooLow { virtue: String, got: u8 },

    #[error("virtue {virtue} > 4 from chargen priority alone (got {got})")]
    VirtueChargenOverFour { virtue: String, got: u8 },

    #[error(
        "background chargen-priority dots total = {got}, expected 7"
    )]
    BackgroundChargenDotsWrong { got: u32 },

    #[error("background {background} has > 3 dots from chargen priority (got {got})")]
    BackgroundChargenOverThree { background: String, got: u8 },

    #[error("Cult background > 2 dots at chargen (got {got})")]
    CultOverTwoAtChargen { got: u8 },

    #[error("charm count = {got}, expected 10 at chargen")]
    CharmCountWrong { got: usize },

    #[error("fewer than 5 caste/favored ability charms (got {got})")]
    CasteFavoredCharmsTooFew { got: usize },

    #[error(
        "charm {charm} requires Ability {ability} >= {required} (character has {got})"
    )]
    CharmAbilityBelowMin {
        charm: String,
        ability: String,
        required: u8,
        got: u8,
    },

    #[error("charm {charm} requires Essence >= {required} (character has {got})")]
    CharmEssenceBelowMin {
        charm: String,
        required: u8,
        got: u8,
    },

    #[error("intimacies = {got} exceeds max ({max} = Willpower + Compassion)")]
    IntimaciesOverMax { got: usize, max: u8 },

    #[error(
        "intimacies = {got} below Compassion baseline {min} (one per Compassion dot required at chargen)"
    )]
    IntimaciesBelowCompassion { got: usize, min: u8 },

    #[error("bonus points spent = {got}, expected 15")]
    BonusPointsWrong { got: u32 },

    #[error(
        "willpower bonus-point purchases push it above 8 without two Virtues >= 4 (got {got})"
    )]
    WillpowerOverEightWithoutHighVirtues { got: u8 },

    #[error("essence > 5 at chargen (got {got})")]
    EssenceOverMaxAtChargen { got: u8 },

    #[error("xp spent ({spent}) exceeds xp earned ({earned})")]
    XpOverspent { spent: u32, earned: u32 },

    #[error(
        "xp purchase for {trait_name} cost {paid}, canonical cost is {expected}"
    )]
    XpCostWrong {
        trait_name: String,
        paid: u32,
        expected: u32,
    },

    #[error("xp banked ({banked}) != xp earned - spent ({earned} - {spent} = {expected})")]
    XpBankedWrong {
        banked: u32,
        earned: u32,
        spent: u32,
        expected: i64,
    },

    #[error("note: charm {charm} not in catalog; prereqs not checked")]
    UnknownCharm { charm: String },
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationReport {
    pub errors: Vec<ValidationError>,
    pub notes: Vec<ValidationError>,
}

impl ValidationReport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn push(&mut self, err: ValidationError) {
        self.errors.push(err);
    }

    pub fn push_note(&mut self, note: ValidationError) {
        self.notes.push(note);
    }

    pub fn extend(&mut self, other: ValidationReport) {
        self.errors.extend(other.errors);
        self.notes.extend(other.notes);
    }
}
