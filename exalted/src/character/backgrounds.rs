use serde::{Deserialize, Serialize};

use super::traits::RatedTrait;

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

/// One instance of a Background. Backgrounds like Allies, Artifact, Mentor,
/// and Backing can be taken multiple times; each occurrence has its own
/// dot rating and free-text label (e.g. "Realm" vs "Guild" for Backing).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackgroundInstance {
    pub kind: BackgroundKind,
    #[serde(default)]
    pub label: String,
    #[serde(rename = "trait")]
    pub trait_: RatedTrait,
    #[serde(default)]
    pub description: String,
}

impl BackgroundInstance {
    pub fn new(kind: BackgroundKind, label: impl Into<String>, trait_: RatedTrait) -> Self {
        Self {
            kind,
            label: label.into(),
            trait_,
            description: String::new(),
        }
    }
}
