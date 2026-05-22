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

    #[error("primary virtue is unset")]
    PrimaryVirtueUnset,

    #[error("virtue flaw is unset")]
    VirtueFlawUnset,

    #[error(
        "virtue flaw {flaw} belongs to {expected_virtue}, but primary virtue is {got_virtue}"
    )]
    VirtueFlawMismatch {
        flaw: String,
        expected_virtue: String,
        got_virtue: String,
    },

    #[error("virtue {virtue} > 4 from chargen priority alone (got {got})")]
    VirtueChargenOverFour { virtue: String, got: u8 },

    #[error(
        "background chargen-priority dots total = {got}, expected 7"
    )]
    BackgroundChargenDotsWrong { got: u32 },

    #[error("background {background} has > 3 dots from chargen priority (got {got})")]
    BackgroundChargenOverThree { background: String, got: u8 },

    #[error("Cult background > 2 dots at creation (chargen + BP) (got {got})")]
    CultOverTwoAtCreation { got: u8 },

    #[error("unknown background id: {id}")]
    UnknownBackgroundId { id: String },

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

    #[error(
        "charm {charm} requires Attribute {attribute} >= {required} (character has {got})"
    )]
    CharmAttributeBelowMin {
        charm: String,
        attribute: String,
        required: u8,
        got: u8,
    },

    #[error("charm {charm} requires at least one {ability} Excellency")]
    CharmPrereqAnyExcellencyMissing { charm: String, ability: String },

    #[error(
        "charm {charm} requires at least {required} {ability} Excellencies (has {got})"
    )]
    CharmPrereqNExcellenciesMissing {
        charm: String,
        ability: String,
        required: u8,
        got: u8,
    },

    #[error("spell {spell} requires the {charm} charm")]
    SpellRequiresSorceryCharm { spell: String, charm: String },

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

    #[error("note: charm {charm} not in rules database; prereqs not checked")]
    UnknownCharm { charm: String },

    #[error("charm {charm} requires prerequisite charm {missing}")]
    CharmPrereqMissing { charm: String, missing: String },

    #[error("willpower base = {got}, expected {expected} (sum of two highest Virtues)")]
    WillpowerNotFromVirtues { expected: u8, got: u8 },

    #[error("willpower > 10 (got {got})")]
    WillpowerOverTen { got: u8 },

    #[error("alien (non-Solar) charm {charm} requires Eclipse caste")]
    AlienCharmOnNonEclipse { charm: String },

    #[error("combo {combo} references charm {charm}, which the character does not own")]
    ComboCharmNotOwned { combo: String, charm: String },

    #[error(
        "combo {combo} includes charm {charm}, which lacks the Combo-Basic or Combo-OK keyword"
    )]
    ComboCharmNotComboable { combo: String, charm: String },

    #[error(
        "combo {combo} mixes a Combo-Basic charm with non-Reflexive charm {charm}"
    )]
    ComboBasicWithNonReflexive { combo: String, charm: String },

    #[error("combo {combo} contains {count} Simple charms (max 1)")]
    ComboMultipleSimple { combo: String, count: usize },

    #[error("combo {combo} repeats charm {charm}")]
    ComboDuplicateCharm { combo: String, charm: String },

    #[error(
        "combo {combo} has an invalid source ({source_kind}); use BonusPoints (chargen) or Xp (in play)"
    )]
    ComboInvalidSource { combo: String, source_kind: String },

    #[error("ability {ability} has {got} distinct specialties; max 3 (Linguistics is exempt)")]
    SpecialtiesOverMax { ability: String, got: usize },

    #[error("ability {ability} specialty {specialty:?} has {got} dots; max 3 (Linguistics is exempt)")]
    SpecialtyOverMaxDots {
        ability: String,
        specialty: String,
        got: u8,
    },

    #[error("multiple native languages declared (got {got}); exactly 1 required")]
    MultipleNativeLanguages { got: usize },

    #[error("no native language declared")]
    NoNativeLanguage,

    #[error("known languages = {got}, exceeds 1 + Linguistics = {max}")]
    TooManyLanguages { got: usize, max: usize },

    #[error("tribal tongues = {got}, exceeds 4 × Linguistics = {max}")]
    TooManyTribalTongues { got: usize, max: usize },

    #[error("Old Realm requires Lore >= 1 (got {lore})")]
    OldRealmRequiresLore { lore: u8 },

    #[error("Guild Cant requires Backing (Guild) >= 2 (got total Backing {backing})")]
    GuildCantRequiresBacking { backing: u8 },

    #[error("duplicate language family {family}")]
    DuplicateLanguageFamily { family: String },

    #[error("native language {family} is missing its dialect specialty (rules p.112)")]
    NativeLanguageMissingDialect { family: String },

    #[error("Ox-Body Technique stacked {got} times, exceeds Resistance cap {max}")]
    OxBodyOverResistance { got: usize, max: u8 },

    #[error("Solar Circle spell {spell} cannot be chosen at character creation")]
    SolarCircleAtChargen { spell: String },

    #[error(
        "Followers {followers} requires Resources, Backing, or Influence \
         >= {followers} to support (max found: {support}) (rules p.114)"
    )]
    FollowersWithoutSupport { followers: u8, support: u8 },

    #[error(
        "bonus-point purchase for {trait_name} cost {paid} BP, canonical cost is {expected}"
    )]
    BpCostWrong {
        trait_name: String,
        paid: u32,
        expected: u32,
    },

    #[error(
        "willpower permanent spent ({spent}) exceeds permanent rating ({permanent})"
    )]
    WillpowerPermanentOverspent { spent: u8, permanent: u8 },

    #[error(
        "willpower temporary ({temporary}) exceeds available permanent dots ({available})"
    )]
    WillpowerTemporaryOverAvailable { temporary: u8, available: u8 },

    #[error(
        "personal essence overspent: spent {spent} + committed {committed} > max {max}"
    )]
    PersonalEssenceOverspent { max: u16, spent: u16, committed: u16 },

    #[error(
        "peripheral essence overspent: spent {spent} + committed {committed} > max {max}"
    )]
    PeripheralEssenceOverspent { max: u16, spent: u16, committed: u16 },

    #[error("hearthstones = {got}, exceeds total Manse background dots {max}")]
    TooManyHearthstones { got: usize, max: u8 },

    #[error("artifacts = {got}, exceeds total Artifact background dots {max}")]
    TooManyArtifacts { got: usize, max: u8 },

    #[error(
        "artifact {artifact} has {socketed} socketed hearthstones but only {sockets} sockets"
    )]
    OversocketedArtifact {
        artifact: String,
        sockets: u8,
        socketed: usize,
    },

    #[error(
        "artifact {artifact} references socketed hearthstone {hearthstone} that does not exist"
    )]
    UnknownSocketedHearthstone {
        artifact: String,
        hearthstone: String,
    },

    #[error(
        "{item} references artifact {artifact_name} that does not exist"
    )]
    MissingLinkedArtifact {
        item: String,
        artifact_name: String,
    },

    #[error("note: {message}")]
    Note { message: String },
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
