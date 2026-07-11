//! In-memory database of charms and spells loaded from `rules/charms.toml`
//! and `rules/spells.toml`. Both files are embedded into the binary via
//! `include_str!`. `init_database()` parses and validates them at startup;
//! `database()` provides lazy access for tests.
//!
//! Validation rules at load time:
//!   - All required fields must be present on every entry (serde-enforced).
//!   - Charm ids must be unique within charms; spell ids must be unique within
//!     spells. (A charm and a spell may share an id, though slug collisions
//!     are unlikely.)
//!
//! The five "universal Excellency" template entries (ids `first-ability-
//! excellency`, `second-ability-excellency`, `third-ability-excellency`,
//! `infinite-ability-mastery`, `ability-essence-flow`) are expanded at load
//! time into 25 derived entries each — one per AbilityKind. The character
//! sheet references the derived ids (e.g. `first-archery-excellency`).

pub mod prereq;

use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::character::{AbilityKind, AttributeKind, BackgroundKind, SpellCircle};

const CHARMS_TOML: &str = include_str!("../../../rules/charms.toml");
const SPELLS_TOML: &str = include_str!("../../../rules/spells.toml");
const BACKGROUNDS_TOML: &str = include_str!("../../../rules/backgrounds.toml");
const ARTS_TOML: &str = include_str!("../../../rules/arts.toml");
const GAME_RULES_MD: &str = include_str!("../../../rules/game_rules.md");
const CHARACTER_CREATION_MD: &str = include_str!("../../../rules/character_creation.md");

/// Embedded text of `rules/game_rules.md` — the core-rules summary document.
pub fn game_rules_markdown() -> &'static str {
    GAME_RULES_MD
}

/// Embedded text of `rules/character_creation.md` — the chargen summary
/// document.
pub fn character_creation_markdown() -> &'static str {
    CHARACTER_CREATION_MD
}

static DATABASE: OnceLock<RulesDatabase> = OnceLock::new();

#[derive(Debug, Error)]
pub enum LoadError {
    #[error("failed to parse charms.toml: {0}")]
    ParseCharms(toml::de::Error),
    #[error("failed to parse spells.toml: {0}")]
    ParseSpells(toml::de::Error),
    #[error("failed to parse backgrounds.toml: {0}")]
    ParseBackgrounds(toml::de::Error),
    #[error("failed to parse arts.toml: {0}")]
    ParseArts(toml::de::Error),
    #[error("duplicate charm id: {0}")]
    DuplicateCharmId(String),
    #[error("duplicate spell id: {0}")]
    DuplicateSpellId(String),
    #[error("duplicate background id: {0}")]
    DuplicateBackgroundId(String),
    #[error("duplicate art id: {0}")]
    DuplicateArtId(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CharmType {
    Reflexive,
    Supplemental,
    Simple,
    #[serde(rename = "Extra Action", alias = "ExtraAction")]
    ExtraAction,
    Permanent,
}

impl CharmType {
    /// Canonical label as printed on the character sheet's TYPE column.
    pub fn display(&self) -> &'static str {
        match self {
            CharmType::Reflexive => "Reflexive",
            CharmType::Supplemental => "Supplemental",
            CharmType::Simple => "Simple",
            CharmType::ExtraAction => "Extra Action",
            CharmType::Permanent => "Permanent",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharmEntry {
    pub id: String,
    pub name: String,
    pub exalt_type: String,
    pub ability: String,
    pub cost: String,
    pub mins_ability: u8,
    pub mins_essence: u8,
    pub charm_type: CharmType,
    pub type_detail: String,
    pub keywords: Vec<String>,
    pub duration: String,
    pub prerequisites: Vec<String>,
    /// Minimum attribute dots required, by attribute. Defaulted empty so the
    /// vast majority of charms (which gate on ability + essence) don't need to
    /// declare it. Authoritative for any charm whose rule-book header table
    /// names a minimum attribute (typically non-Solar splats).
    #[serde(default)]
    pub mins_attribute: BTreeMap<AttributeKind, u8>,
    pub source: String,
    pub pages: String,
    /// One-line summary suitable for the EFFECT column on the printed
    /// character sheet (~25–40 characters reads cleanly; longer is
    /// allowed and AcroForm auto-shrinks).
    pub effect: String,
    pub description: String,
}

impl CharmEntry {
    /// Parse the `ability` string into a concrete `AbilityKind` if possible.
    /// Returns `None` for `"(any)"`, empty strings, or anything that doesn't
    /// map to a known ability (e.g. Dragon-Blooded Anima Powers). Handles
    /// both `"martial arts"` and the typo `"martial-arts"`.
    pub fn ability_kind(&self) -> Option<AbilityKind> {
        parse_ability_kind(&self.ability)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpellEntry {
    pub id: String,
    pub name: String,
    pub circle: SpellCircle,
    pub cost: String,
    pub keywords: Vec<String>,
    pub duration: String,
    pub target: String,
    pub source: String,
    pub pages: String,
    /// One-line summary for the EFFECT column on the printed sheet.
    pub effect: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackgroundEntry {
    pub id: String,
    pub name: String,
    pub kind: BackgroundKind,
    pub source: String,
    pub pages: String,
    pub description: String,
}

/// One extra Ability minimum a Thaumaturgy Art imposes beyond the universal
/// Occult ladder (Initiate→Occult 1, Adept→Occult 3, Master→Occult 5). Read as:
/// "to hold Degree ≥ `degree` in this Art, the character needs `ability` ≥
/// `min`." A requirement that scales per-Degree (e.g. Alchemy's Craft(Water) 1
/// per Degree) is encoded as one `ArtRequirement` per Degree level.
///
/// `focus` disambiguates a family ability — for Craft, the craft focus (e.g.
/// "Water"); ignored for other abilities. When set, the requirement is met by a
/// craft of that focus rated ≥ `min`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtRequirement {
    pub ability: AbilityKind,
    pub min: u8,
    /// Lowest Degree (1..=3) this requirement applies from.
    pub degree: u8,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub focus: String,
}

/// One Art of Thaumaturgy in the rules database. Requirements beyond the
/// universal Occult ladder are listed in `requirements`; anything that resists
/// structuring stays in `description`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtEntry {
    pub id: String,
    pub name: String,
    pub source: String,
    pub pages: String,
    #[serde(default)]
    pub requirements: Vec<ArtRequirement>,
    pub description: String,
}

#[derive(Debug, Deserialize)]
struct CharmsFile {
    #[serde(default)]
    charm: Vec<CharmEntry>,
}

#[derive(Debug, Deserialize)]
struct SpellsFile {
    #[serde(default)]
    spell: Vec<SpellEntry>,
}

#[derive(Debug, Deserialize)]
struct BackgroundsFile {
    #[serde(default)]
    background: Vec<BackgroundEntry>,
}

#[derive(Debug, Deserialize)]
struct ArtsFile {
    #[serde(default)]
    art: Vec<ArtEntry>,
}

#[derive(Debug, Clone)]
pub struct RulesDatabase {
    charms: HashMap<String, CharmEntry>,
    spells: HashMap<String, SpellEntry>,
    backgrounds: HashMap<String, BackgroundEntry>,
    arts: HashMap<String, ArtEntry>,
}

impl RulesDatabase {
    pub fn from_embedded() -> Result<Self, LoadError> {
        Self::from_strings(CHARMS_TOML, SPELLS_TOML, BACKGROUNDS_TOML, ARTS_TOML)
    }

    pub fn from_strings(
        charms_toml: &str,
        spells_toml: &str,
        backgrounds_toml: &str,
        arts_toml: &str,
    ) -> Result<Self, LoadError> {
        let charms_file: CharmsFile =
            toml::from_str(charms_toml).map_err(LoadError::ParseCharms)?;
        let spells_file: SpellsFile =
            toml::from_str(spells_toml).map_err(LoadError::ParseSpells)?;
        let backgrounds_file: BackgroundsFile =
            toml::from_str(backgrounds_toml).map_err(LoadError::ParseBackgrounds)?;
        let arts_file: ArtsFile = toml::from_str(arts_toml).map_err(LoadError::ParseArts)?;

        let expanded_charms = expand_excellency_templates(charms_file.charm);

        let mut charms = HashMap::new();
        for entry in expanded_charms {
            if charms.contains_key(&entry.id) {
                return Err(LoadError::DuplicateCharmId(entry.id));
            }
            charms.insert(entry.id.clone(), entry);
        }

        let mut spells = HashMap::new();
        for entry in spells_file.spell {
            if spells.contains_key(&entry.id) {
                return Err(LoadError::DuplicateSpellId(entry.id));
            }
            spells.insert(entry.id.clone(), entry);
        }

        let mut backgrounds = HashMap::new();
        for entry in backgrounds_file.background {
            if backgrounds.contains_key(&entry.id) {
                return Err(LoadError::DuplicateBackgroundId(entry.id));
            }
            backgrounds.insert(entry.id.clone(), entry);
        }

        let mut arts = HashMap::new();
        for entry in arts_file.art {
            if arts.contains_key(&entry.id) {
                return Err(LoadError::DuplicateArtId(entry.id));
            }
            arts.insert(entry.id.clone(), entry);
        }

        Ok(Self {
            charms,
            spells,
            backgrounds,
            arts,
        })
    }

    pub fn charm(&self, id: &str) -> Option<&CharmEntry> {
        self.charms.get(id)
    }

    pub fn spell(&self, id: &str) -> Option<&SpellEntry> {
        self.spells.get(id)
    }

    pub fn background(&self, id: &str) -> Option<&BackgroundEntry> {
        self.backgrounds.get(id)
    }

    pub fn art(&self, id: &str) -> Option<&ArtEntry> {
        self.arts.get(id)
    }

    pub fn charm_count(&self) -> usize {
        self.charms.len()
    }

    pub fn spell_count(&self) -> usize {
        self.spells.len()
    }

    pub fn background_count(&self) -> usize {
        self.backgrounds.len()
    }

    pub fn art_count(&self) -> usize {
        self.arts.len()
    }

    pub fn iter_charms(&self) -> impl Iterator<Item = &CharmEntry> {
        self.charms.values()
    }

    pub fn iter_spells(&self) -> impl Iterator<Item = &SpellEntry> {
        self.spells.values()
    }

    pub fn iter_backgrounds(&self) -> impl Iterator<Item = &BackgroundEntry> {
        self.backgrounds.values()
    }

    pub fn iter_arts(&self) -> impl Iterator<Item = &ArtEntry> {
        self.arts.values()
    }
}

/// Eagerly initialize the global database. Call from `main()` at startup so
/// that load errors fail fast with a clear message instead of panicking at
/// the first lookup.
pub fn init_database() -> Result<&'static RulesDatabase, LoadError> {
    let db = RulesDatabase::from_embedded()?;
    Ok(DATABASE.get_or_init(|| db))
}

/// Get the global database, parsing the embedded TOML on first access if
/// `init_database` hasn't been called. Panics if the embedded data is
/// malformed — production code paths should call `init_database` first.
pub fn database() -> &'static RulesDatabase {
    DATABASE.get_or_init(|| {
        RulesDatabase::from_embedded()
            .expect("embedded rules database is corrupt; run `init_database` for a clean error")
    })
}

// --------------------------------------------------------------------------
// Excellency template expansion
// --------------------------------------------------------------------------

/// The five template entries that should be expanded to 25 per-ability copies
/// at load time. Each template has `ability = "(any)"` and a `(Ability)`
/// placeholder in its name and description.
const EXCELLENCY_TEMPLATE_IDS: &[&str] = &[
    "first-ability-excellency",
    "second-ability-excellency",
    "third-ability-excellency",
    "infinite-ability-mastery",
    "ability-essence-flow",
];

fn expand_excellency_templates(entries: Vec<CharmEntry>) -> Vec<CharmEntry> {
    let mut out = Vec::with_capacity(entries.len() + 120);
    for entry in entries {
        if EXCELLENCY_TEMPLATE_IDS.contains(&entry.id.as_str()) {
            for &ability in AbilityKind::ALL {
                out.push(specialize_excellency(&entry, ability));
            }
        } else {
            out.push(entry);
        }
    }
    out
}

fn specialize_excellency(template: &CharmEntry, ability: AbilityKind) -> CharmEntry {
    let kebab = ability_kebab_name(ability);
    let display = ability_display_name(ability);
    // Substitute "ability" → kebab into each prereq string so that templated
    // prereqs like "any-ability-excellency" become "any-archery-excellency"
    // on the derived entry. Without this, the resolver can never match.
    let prerequisites = template
        .prerequisites
        .iter()
        .map(|p| p.replace("ability", kebab))
        .collect();
    CharmEntry {
        id: template.id.replace("ability", kebab),
        name: template.name.replace("(Ability)", display),
        ability: display.to_string(),
        description: template.description.replace("(Ability)", display),
        effect: template.effect.replace("(Ability)", display),
        // Everything else copies through unchanged.
        exalt_type: template.exalt_type.clone(),
        cost: template.cost.clone(),
        mins_ability: template.mins_ability,
        mins_essence: template.mins_essence,
        charm_type: template.charm_type,
        type_detail: template.type_detail.clone(),
        keywords: template.keywords.clone(),
        duration: template.duration.clone(),
        prerequisites,
        mins_attribute: template.mins_attribute.clone(),
        source: template.source.clone(),
        pages: template.pages.clone(),
    }
}

// --------------------------------------------------------------------------
// Ability helpers
// --------------------------------------------------------------------------

/// Human-readable name for an Ability, used when rendering and when
/// substituting into Excellency template names ("First Martial Arts
/// Excellency: Essence Overwhelming").
pub fn ability_display_name(ability: AbilityKind) -> &'static str {
    match ability {
        AbilityKind::Archery => "Archery",
        AbilityKind::MartialArts => "Martial Arts",
        AbilityKind::Melee => "Melee",
        AbilityKind::Thrown => "Thrown",
        AbilityKind::War => "War",
        AbilityKind::Integrity => "Integrity",
        AbilityKind::Performance => "Performance",
        AbilityKind::Presence => "Presence",
        AbilityKind::Resistance => "Resistance",
        AbilityKind::Survival => "Survival",
        AbilityKind::Craft => "Craft",
        AbilityKind::Investigation => "Investigation",
        AbilityKind::Lore => "Lore",
        AbilityKind::Medicine => "Medicine",
        AbilityKind::Occult => "Occult",
        AbilityKind::Athletics => "Athletics",
        AbilityKind::Awareness => "Awareness",
        AbilityKind::Dodge => "Dodge",
        AbilityKind::Larceny => "Larceny",
        AbilityKind::Stealth => "Stealth",
        AbilityKind::Bureaucracy => "Bureaucracy",
        AbilityKind::Linguistics => "Linguistics",
        AbilityKind::Ride => "Ride",
        AbilityKind::Sail => "Sail",
        AbilityKind::Socialize => "Socialize",
    }
}

/// Kebab-case slug for an Ability, used when substituting into Excellency
/// template ids ("first-martial-arts-excellency").
pub fn ability_kebab_name(ability: AbilityKind) -> &'static str {
    match ability {
        AbilityKind::Archery => "archery",
        AbilityKind::MartialArts => "martial-arts",
        AbilityKind::Melee => "melee",
        AbilityKind::Thrown => "thrown",
        AbilityKind::War => "war",
        AbilityKind::Integrity => "integrity",
        AbilityKind::Performance => "performance",
        AbilityKind::Presence => "presence",
        AbilityKind::Resistance => "resistance",
        AbilityKind::Survival => "survival",
        AbilityKind::Craft => "craft",
        AbilityKind::Investigation => "investigation",
        AbilityKind::Lore => "lore",
        AbilityKind::Medicine => "medicine",
        AbilityKind::Occult => "occult",
        AbilityKind::Athletics => "athletics",
        AbilityKind::Awareness => "awareness",
        AbilityKind::Dodge => "dodge",
        AbilityKind::Larceny => "larceny",
        AbilityKind::Stealth => "stealth",
        AbilityKind::Bureaucracy => "bureaucracy",
        AbilityKind::Linguistics => "linguistics",
        AbilityKind::Ride => "ride",
        AbilityKind::Sail => "sail",
        AbilityKind::Socialize => "socialize",
    }
}

fn parse_ability_kind(raw: &str) -> Option<AbilityKind> {
    let normalized = raw.trim().to_lowercase().replace('-', " ");
    for &ability in AbilityKind::ALL {
        if ability_display_name(ability).to_lowercase() == normalized {
            return Some(ability);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_data_loads_cleanly() {
        let db = RulesDatabase::from_embedded().expect("embedded TOMLs should parse");
        // After Excellency expansion: 272 source - 5 templates + 5*25 derived = 392.
        assert!(db.charm_count() >= 380, "got {} charms", db.charm_count());
        assert_eq!(db.spell_count(), 223, "got {} spells", db.spell_count());
        assert_eq!(
            db.background_count(),
            11,
            "got {} backgrounds",
            db.background_count()
        );
        assert_eq!(db.art_count(), 11, "got {} arts", db.art_count());
        // Every BackgroundKind variant should be represented by at least one
        // entry whose `kind` matches.
        for &kind in BackgroundKind::ALL {
            assert!(
                db.iter_backgrounds().any(|b| b.kind == kind),
                "no background entry for kind {:?}",
                kind
            );
        }
    }

    #[test]
    fn excellency_templates_are_expanded() {
        let db = RulesDatabase::from_embedded().unwrap();
        // The template ids themselves must not survive.
        for tpl in EXCELLENCY_TEMPLATE_IDS {
            assert!(
                db.charm(tpl).is_none(),
                "template id {tpl} should have been expanded away"
            );
        }
        // Every (template × ability) variant must exist.
        for &ability in AbilityKind::ALL {
            let kebab = ability_kebab_name(ability);
            for tpl in EXCELLENCY_TEMPLATE_IDS {
                let derived = tpl.replace("ability", kebab);
                let entry = db
                    .charm(&derived)
                    .unwrap_or_else(|| panic!("missing derived id {derived}"));
                let display = ability_display_name(ability);
                assert!(
                    entry.name.contains(display) || entry.name == display,
                    "derived name {:?} should contain {:?}",
                    entry.name,
                    display
                );
                assert!(
                    !entry.name.contains("(Ability)"),
                    "placeholder left in name: {:?}",
                    entry.name
                );
                assert!(
                    !entry.description.contains("(Ability)"),
                    "placeholder left in description for {}",
                    derived
                );
                assert_eq!(entry.ability, display);
            }
        }
    }

    #[test]
    fn duplicate_charm_id_rejected() {
        let dup = r#"
[[charm]]
id = "x"
name = "X"
exalt_type = "solar"
ability = "lore"
cost = "0m"
mins_ability = 1
mins_essence = 1
charm_type = "Reflexive"
type_detail = ""
keywords = []
duration = "Instant"
prerequisites = []
source = "test"
pages = "1"
effect = "test"
description = "test"

[[charm]]
id = "x"
name = "X again"
exalt_type = "solar"
ability = "lore"
cost = "0m"
mins_ability = 1
mins_essence = 1
charm_type = "Reflexive"
type_detail = ""
keywords = []
duration = "Instant"
prerequisites = []
source = "test"
pages = "1"
effect = "test"
description = "test"
"#;
        let err = RulesDatabase::from_strings(dup, "", "", "").unwrap_err();
        assert!(matches!(err, LoadError::DuplicateCharmId(ref id) if id == "x"));
    }

    #[test]
    fn duplicate_background_id_rejected() {
        let dup = r#"
[[background]]
id = "bg-x"
name = "X"
kind = "Allies"
source = "test"
pages = "1"
description = "test"

[[background]]
id = "bg-x"
name = "X again"
kind = "Allies"
source = "test"
pages = "1"
description = "test"
"#;
        let err = RulesDatabase::from_strings("", "", dup, "").unwrap_err();
        assert!(matches!(err, LoadError::DuplicateBackgroundId(ref id) if id == "bg-x"));
    }

    #[test]
    fn duplicate_art_id_rejected() {
        let dup = r#"
[[art]]
id = "art-x"
name = "X"
source = "test"
pages = "1"
description = "test"

[[art]]
id = "art-x"
name = "X again"
source = "test"
pages = "1"
description = "test"
"#;
        let err = RulesDatabase::from_strings("", "", "", dup).unwrap_err();
        assert!(matches!(err, LoadError::DuplicateArtId(ref id) if id == "art-x"));
    }

    #[test]
    fn missing_required_field_rejected() {
        // Missing `description`.
        let bad = r#"
[[charm]]
id = "x"
name = "X"
exalt_type = "solar"
ability = "lore"
cost = "0m"
mins_ability = 1
mins_essence = 1
charm_type = "Reflexive"
type_detail = ""
keywords = []
duration = "Instant"
prerequisites = []
source = "test"
pages = "1"
"#;
        let err = RulesDatabase::from_strings(bad, "", "", "").unwrap_err();
        assert!(matches!(err, LoadError::ParseCharms(_)));
    }

    #[test]
    fn parse_ability_handles_variants() {
        assert_eq!(parse_ability_kind("archery"), Some(AbilityKind::Archery));
        assert_eq!(
            parse_ability_kind("martial arts"),
            Some(AbilityKind::MartialArts)
        );
        assert_eq!(
            parse_ability_kind("martial-arts"),
            Some(AbilityKind::MartialArts)
        );
        assert_eq!(parse_ability_kind("(any)"), None);
        assert_eq!(parse_ability_kind(""), None);
    }
}
