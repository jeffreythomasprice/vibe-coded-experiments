//! Thaumaturgy — the "occult arts" open to mortals and Exalts alike. Unlike
//! Sorcery (gated behind the `terrestrial-circle-sorcery` charm), thaumaturgy
//! needs no charm or merit: the only entry requirement is Occult ≥ 1. A
//! character learns individual **Arts** (Alchemy, Astrology, …), each of which
//! can be advanced through three **Degrees** — Initiate (+1), Adept (+2),
//! Master (+3) — where the Degree is the specialty-dice bonus it grants to
//! thaumaturgy rolls. See `rules/character_creation.md` and Oadenol's Codex.
//!
//! Modeling notes:
//! - A Degree is a paid step exactly like an Ability dot, so an Art's level is
//!   a [`RatedTrait`] whose `dots()` (0..=3) is the Degree. This reuses every
//!   `DotSource` accounting helper, mirroring how [`super::traits::Craft`] and
//!   [`super::backgrounds::BackgroundRef`] wrap a `RatedTrait`.
//! - [`Procedure`]s are single memorized rituals bought à la carte (1 XP each,
//!   3 per bonus point). A character may hold Procedures in an Art with no
//!   Degree in it, so `rating` can be 0 while `procedures` is non-empty.

use serde::{Deserialize, Serialize};

use super::notes::Note;
use super::traits::{DotSource, RatedTrait};
use crate::rules::database::{ArtEntry, RulesDatabase};

/// One Art of Thaumaturgy a character has taken, referenced by database id.
/// `rating` holds the Degree (0..=3) with per-Degree purchase provenance;
/// `procedures` holds any à-la-carte rituals learned within the Art.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OccultArt {
    pub id: String,
    /// Degree of mastery, 0..=3. `dots()` == the Degree == the thaumaturgy
    /// specialty-dice bonus this Art grants.
    #[serde(rename = "degree")]
    pub rating: RatedTrait,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub procedures: Vec<Procedure>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<Note>,
}

impl OccultArt {
    /// A newly-referenced Art at Degree 0 (no purchases). Advance it by pushing
    /// Degree purchases onto `rating` via the usual `RatedTrait` helpers.
    pub fn lookup(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            rating: RatedTrait::with_base(0),
            procedures: Vec::new(),
            notes: Vec::new(),
        }
    }

    /// Append a fresh note and return self for chaining.
    pub fn with_notes(mut self, body: impl Into<String>) -> Self {
        self.notes.push(Note::new(body));
        self
    }

    pub fn push_note(&mut self, body: impl Into<String>) {
        self.notes.push(Note::new(body));
    }

    pub fn notes(&self) -> &[Note] {
        &self.notes
    }

    /// The Degree of mastery (0..=3).
    pub fn degree(&self) -> u8 {
        self.rating.dots()
    }

    /// Resolve to the underlying [`ArtEntry`]. Returns `None` when the id isn't
    /// in the database (chargen validation flags this as `UnknownArtId`).
    pub fn entry<'a>(&self, db: &'a RulesDatabase) -> Option<&'a ArtEntry> {
        db.art(&self.id)
    }

    /// Display name, falling back to the id when it can't be resolved.
    pub fn display_name<'a>(&'a self, db: &'a RulesDatabase) -> &'a str {
        match self.entry(db) {
            Some(entry) => &entry.name,
            None => &self.id,
        }
    }
}

/// A single memorized thaumaturgical ritual within an Art. Grants no bonus
/// dice; `degree` is the Degree-rank (0..=3) the ritual emulates. `source`
/// carries BP/XP provenance exactly like every other purchase — a Procedure
/// costs 1 XP post-chargen or 1/3 of a bonus point at creation (3 per BP).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Procedure {
    pub name: String,
    /// Degree-rank the ritual emulates (0 = Apprentice … 3 = Master).
    #[serde(default)]
    pub degree: u8,
    pub source: DotSource,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<Note>,
}

impl Procedure {
    pub fn new(name: impl Into<String>, degree: u8, source: DotSource) -> Self {
        Self {
            name: name.into(),
            degree,
            source,
            notes: Vec::new(),
        }
    }
}
