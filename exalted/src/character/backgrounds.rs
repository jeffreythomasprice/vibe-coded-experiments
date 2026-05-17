use serde::{Deserialize, Serialize};

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
