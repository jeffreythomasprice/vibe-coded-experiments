use serde::{Deserialize, Serialize};

/// A language a character knows. The named families are the canonical
/// 1-per-Linguistics-dot families (p.111-112). `TribalTongue(name)` is a
/// distinct category: each Linguistics dot grants four of them, separately
/// from the per-family count.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
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
    /// A single tribal tongue, named (e.g. "Marukan", "Delzahn"). Does not
    /// count against the per-Linguistics family cap; capped instead at
    /// `4 × Linguistics` total.
    TribalTongue(String),
}

impl LanguageFamily {
    /// All non-tribal families (the fixed enum variants). Tribal tongues are
    /// named at runtime and not part of this list.
    pub const NAMED: &'static [LanguageFamily] = &[
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

    pub fn is_tribal(&self) -> bool {
        matches!(self, LanguageFamily::TribalTongue(_))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnownLanguage {
    pub family: LanguageFamily,
    pub dialect_specialty: Option<String>,
    pub native: bool,
}
