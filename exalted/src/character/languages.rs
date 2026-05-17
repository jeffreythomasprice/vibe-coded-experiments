use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub enum LanguageFamily {
    HighRealm,
    LowRealm,
    OldRealm,
    Riverspeak,
    Skytongue,
    Flametongue,
    Seatongue,
    ForestTongue,
    GuildCant,
}

impl LanguageFamily {
    pub const ALL: &'static [LanguageFamily] = &[
        LanguageFamily::HighRealm,
        LanguageFamily::LowRealm,
        LanguageFamily::OldRealm,
        LanguageFamily::Riverspeak,
        LanguageFamily::Skytongue,
        LanguageFamily::Flametongue,
        LanguageFamily::Seatongue,
        LanguageFamily::ForestTongue,
        LanguageFamily::GuildCant,
    ];
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnownLanguage {
    pub family: LanguageFamily,
    pub dialect_specialty: Option<String>,
    pub native: bool,
}
