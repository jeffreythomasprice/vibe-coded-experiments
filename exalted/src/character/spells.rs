use serde::{Deserialize, Serialize};

use super::notes::Note;
use super::traits::DotSource;
use crate::rules::database::{RulesDatabase, SpellEntry};

/// Sorcery circle for a known spell (p.77, §6.2 of `character_creation.md`).
/// Solars may learn Terrestrial and Celestial spells at character creation
/// via the "swap a Charm for a spell" exchange, but **Solar Circle spells
/// are forbidden at creation** and must be learnt post-chargen with XP.
///
/// Necromancy circles (`Shadowlands`, `Void`, `Labyrinth`) are included so
/// that the rules database can store necromantic spells; chargen rules for
/// necromancy are not yet enforced beyond inheriting the same Solar-style
/// gate where applicable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpellCircle {
    Terrestrial,
    Celestial,
    Solar,
    Shadowlands,
    Void,
    Labyrinth,
}

impl SpellCircle {
    pub const ALL: &'static [SpellCircle] = &[
        SpellCircle::Terrestrial,
        SpellCircle::Celestial,
        SpellCircle::Solar,
        SpellCircle::Shadowlands,
        SpellCircle::Void,
        SpellCircle::Labyrinth,
    ];
}

/// A single sorcery spell on a character sheet. Either a reference to an
/// entry in the rules database by id (with optional descriptive notes), or a
/// one-off custom spell whose full definition lives inline.
///
/// `source` mirrors `CharmRef.source`: a spell taken via the chargen sorcery
/// swap occupies one of the 10 Charm picks (`ChargenPriority`), a BP-bought
/// spell costs the same as a Charm (`BonusPoints`), and a post-chargen spell
/// uses XP at the `xp_cost_spell` rate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum SpellRef {
    Lookup {
        id: String,
        source: DotSource,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        notes: Vec<Note>,
    },
    Custom {
        entry: SpellEntry,
        source: DotSource,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        notes: Vec<Note>,
    },
}

impl SpellRef {
    pub fn lookup(id: impl Into<String>, source: DotSource) -> Self {
        Self::Lookup {
            id: id.into(),
            source,
            notes: Vec::new(),
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

    pub fn notes(&self) -> &[Note] {
        match self {
            Self::Lookup { notes, .. } | Self::Custom { notes, .. } => notes,
        }
    }

    /// Lookup id (None for Custom spells).
    pub fn id(&self) -> Option<&str> {
        match self {
            Self::Lookup { id, .. } => Some(id),
            Self::Custom { .. } => None,
        }
    }

    /// Resolve to the underlying `SpellEntry` — either from the database (for
    /// Lookup with a known id) or inline (for Custom). Returns `None` only
    /// when a Lookup references an id that isn't in the database.
    pub fn entry<'a>(&'a self, db: &'a RulesDatabase) -> Option<&'a SpellEntry> {
        match self {
            Self::Lookup { id, .. } => db.spell(id),
            Self::Custom { entry, .. } => Some(entry),
        }
    }

    /// Display name, falling back to the id when a Lookup can't be resolved.
    pub fn display_name<'a>(&'a self, db: &'a RulesDatabase) -> &'a str {
        match self.entry(db) {
            Some(entry) => &entry.name,
            None => match self {
                Self::Lookup { id, .. } => id,
                Self::Custom { entry, .. } => &entry.name,
            },
        }
    }

    /// Spell circle from the resolved entry, or `None` if a Lookup id is
    /// unknown.
    pub fn circle(&self, db: &RulesDatabase) -> Option<SpellCircle> {
        self.entry(db).map(|e| e.circle)
    }
}
