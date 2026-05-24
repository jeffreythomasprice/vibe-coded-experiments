use serde::{Deserialize, Serialize};

use super::notes::Note;
use super::traits::{AbilityKind, DotSource};
use crate::rules::database::{CharmEntry, RulesDatabase};
use crate::rules::health::OxBodyPattern;

/// A single charm on a character sheet. Either a reference to an entry in
/// the rules database by id (with optional descriptive notes), or a one-off
/// custom charm whose full definition lives inline.
///
/// `source` tracks how the slot was paid for: a charm taken via chargen
/// priority occupies one of the 10 picks (`ChargenPriority`); a BP-purchased
/// charm costs the same as a Charm slot (`BonusPoints`); a post-chargen
/// charm uses XP at the rate from `xp_costs::xp_cost_charm`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum CharmRef {
    Lookup {
        id: String,
        source: DotSource,
        #[serde(default)]
        non_solar: bool,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        notes: Vec<Note>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ox_body_pattern: Option<OxBodyPattern>,
    },
    Custom {
        entry: CharmEntry,
        source: DotSource,
        #[serde(default)]
        non_solar: bool,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        notes: Vec<Note>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ox_body_pattern: Option<OxBodyPattern>,
    },
}

impl CharmRef {
    pub fn lookup(id: impl Into<String>, source: DotSource) -> Self {
        Self::Lookup {
            id: id.into(),
            source,
            non_solar: false,
            notes: Vec::new(),
            ox_body_pattern: None,
        }
    }

    /// Append a fresh note and return self for chaining.
    pub fn with_notes(mut self, body: impl Into<String>) -> Self {
        self.push_note(body);
        self
    }

    pub fn push_note(&mut self, body: impl Into<String>) {
        let n = Note::new(body);
        match self {
            Self::Lookup { notes, .. } | Self::Custom { notes, .. } => notes.push(n),
        }
    }

    pub fn source(&self) -> DotSource {
        match self {
            Self::Lookup { source, .. } | Self::Custom { source, .. } => *source,
        }
    }

    pub fn non_solar(&self) -> bool {
        match self {
            Self::Lookup { non_solar, .. } | Self::Custom { non_solar, .. } => *non_solar,
        }
    }

    pub fn notes(&self) -> &[Note] {
        match self {
            Self::Lookup { notes, .. } | Self::Custom { notes, .. } => notes,
        }
    }

    pub fn ox_body_pattern(&self) -> Option<OxBodyPattern> {
        match self {
            Self::Lookup { ox_body_pattern, .. }
            | Self::Custom { ox_body_pattern, .. } => *ox_body_pattern,
        }
    }

    /// The lookup id (for `Lookup` variants) or the embedded entry's id
    /// (for `Custom` variants). Always present.
    pub fn id(&self) -> &str {
        match self {
            Self::Lookup { id, .. } => id,
            Self::Custom { entry, .. } => &entry.id,
        }
    }

    /// Convenience: true when this charm references the given id, whether
    /// via Lookup or via the embedded Custom entry.
    pub fn is_id(&self, target: &str) -> bool {
        self.id() == target
    }

    /// Resolve to the underlying `CharmEntry` — from the database for
    /// `Lookup`, inline for `Custom`. Returns `None` only when a `Lookup`
    /// references an id that isn't in the database.
    pub fn entry<'a>(&'a self, db: &'a RulesDatabase) -> Option<&'a CharmEntry> {
        match self {
            Self::Lookup { id, .. } => db.charm(id),
            Self::Custom { entry, .. } => Some(entry),
        }
    }

    /// Display name, falling back to the id when a Lookup is unresolved.
    pub fn display_name<'a>(&'a self, db: &'a RulesDatabase) -> &'a str {
        match self.entry(db) {
            Some(entry) => &entry.name,
            None => self.id(),
        }
    }

    /// Resolved ability for this charm, parsed from the entry's `ability`
    /// field. Returns `None` if the lookup id is unknown, or if the entry's
    /// ability is a wildcard (`"(any)"`) or empty (e.g. anima powers).
    pub fn ability(&self, db: &RulesDatabase) -> Option<AbilityKind> {
        self.entry(db).and_then(|e| e.ability_kind())
    }
}
