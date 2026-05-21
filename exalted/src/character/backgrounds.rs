use serde::{Deserialize, Serialize};

use super::traits::RatedTrait;
use crate::rules::database::{BackgroundEntry, RulesDatabase};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub enum BackgroundKind {
    Allies,
    Artifact,
    Backing,
    Contacts,
    Cult,
    Familiar,
    Followers,
    Influence,
    Manse,
    Mentor,
    Resources,
}

impl BackgroundKind {
    pub const ALL: &'static [BackgroundKind] = &[
        BackgroundKind::Allies,
        BackgroundKind::Artifact,
        BackgroundKind::Backing,
        BackgroundKind::Contacts,
        BackgroundKind::Cult,
        BackgroundKind::Familiar,
        BackgroundKind::Followers,
        BackgroundKind::Influence,
        BackgroundKind::Manse,
        BackgroundKind::Mentor,
        BackgroundKind::Resources,
    ];
}

/// Canonical database id for each core background kind. The 11 entries in
/// `rules/backgrounds.toml` use these exact ids; tests and constructors that
/// only care about the kind can use [`BackgroundRef::lookup_kind`] to avoid
/// hand-typing them.
pub fn canonical_id(kind: BackgroundKind) -> &'static str {
    match kind {
        BackgroundKind::Allies => "allies",
        BackgroundKind::Artifact => "artifact",
        BackgroundKind::Backing => "backing",
        BackgroundKind::Contacts => "contacts",
        BackgroundKind::Cult => "cult",
        BackgroundKind::Familiar => "familiar",
        BackgroundKind::Followers => "followers",
        BackgroundKind::Influence => "influence",
        BackgroundKind::Manse => "manse",
        BackgroundKind::Mentor => "mentor",
        BackgroundKind::Resources => "resources",
    }
}

/// One Background a character has taken — either a reference to an entry in
/// the rules database by id, or a one-off custom background whose full entry
/// lives inline.
///
/// `label` is the short distinguisher used when the same kind is taken more
/// than once (e.g. `"Realm"` vs `"Guild"` for Backing, or the name of a
/// specific artifact). `notes` is free-form text describing this particular
/// instance — what the artifact is, who the mentor is, etc.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind_tag", rename_all = "lowercase")]
pub enum BackgroundRef {
    Lookup {
        id: String,
        #[serde(rename = "trait")]
        trait_: RatedTrait,
        #[serde(default)]
        label: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        notes: Option<String>,
    },
    Custom {
        entry: BackgroundEntry,
        #[serde(rename = "trait")]
        trait_: RatedTrait,
        #[serde(default)]
        label: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        notes: Option<String>,
    },
}

impl BackgroundRef {
    pub fn lookup(id: impl Into<String>, trait_: RatedTrait) -> Self {
        Self::Lookup {
            id: id.into(),
            trait_,
            label: String::new(),
            notes: None,
        }
    }

    /// Convenience constructor for one of the 11 core backgrounds.
    pub fn lookup_kind(kind: BackgroundKind, trait_: RatedTrait) -> Self {
        Self::lookup(canonical_id(kind), trait_)
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        match &mut self {
            Self::Lookup { label: l, .. } | Self::Custom { label: l, .. } => *l = label.into(),
        }
        self
    }

    pub fn with_notes(mut self, notes: impl Into<String>) -> Self {
        let n = Some(notes.into());
        match &mut self {
            Self::Lookup { notes, .. } | Self::Custom { notes, .. } => *notes = n,
        }
        self
    }

    /// Lookup id (None for Custom).
    pub fn id(&self) -> Option<&str> {
        match self {
            Self::Lookup { id, .. } => Some(id),
            Self::Custom { .. } => None,
        }
    }

    pub fn trait_(&self) -> &RatedTrait {
        match self {
            Self::Lookup { trait_, .. } | Self::Custom { trait_, .. } => trait_,
        }
    }

    pub fn trait_mut(&mut self) -> &mut RatedTrait {
        match self {
            Self::Lookup { trait_, .. } | Self::Custom { trait_, .. } => trait_,
        }
    }

    pub fn label(&self) -> &str {
        match self {
            Self::Lookup { label, .. } | Self::Custom { label, .. } => label,
        }
    }

    pub fn notes(&self) -> Option<&str> {
        match self {
            Self::Lookup { notes, .. } | Self::Custom { notes, .. } => notes.as_deref(),
        }
    }

    /// Resolve to the underlying `BackgroundEntry` — either from the database
    /// (for Lookup with a known id) or inline (for Custom). Returns `None`
    /// only when a Lookup references an id that isn't in the database.
    pub fn entry<'a>(&'a self, db: &'a RulesDatabase) -> Option<&'a BackgroundEntry> {
        match self {
            Self::Lookup { id, .. } => db.background(id),
            Self::Custom { entry, .. } => Some(entry),
        }
    }

    /// Kind of background — derived from the resolved entry. Returns `None`
    /// when a Lookup id can't be resolved.
    pub fn kind(&self, db: &RulesDatabase) -> Option<BackgroundKind> {
        self.entry(db).map(|e| e.kind)
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
}
