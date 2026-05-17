use std::collections::BTreeSet;

use crate::character::{AbilityKind, BackgroundKind, Character, LanguageFamily};
use crate::error::{ValidationError, ValidationReport};

pub fn validate_languages(c: &Character, report: &mut ValidationReport) {
    let native_count = c.languages.iter().filter(|l| l.native).count();
    match native_count {
        0 => report.push(ValidationError::NoNativeLanguage),
        1 => {}
        n => report.push(ValidationError::MultipleNativeLanguages { got: n }),
    }

    let linguistics = c.ability(AbilityKind::Linguistics);
    let max_families = 1 + linguistics as usize;
    if c.languages.len() > max_families {
        report.push(ValidationError::TooManyLanguages {
            got: c.languages.len(),
            max: max_families,
        });
    }

    let max_tribal = linguistics.saturating_mul(4);
    if c.tribal_tongues > max_tribal {
        report.push(ValidationError::TooManyTribalTongues {
            got: c.tribal_tongues,
            max: max_tribal,
        });
    }

    let mut seen: BTreeSet<LanguageFamily> = BTreeSet::new();
    for lang in &c.languages {
        if !seen.insert(lang.family) {
            report.push(ValidationError::DuplicateLanguageFamily {
                family: format!("{:?}", lang.family),
            });
        }
    }

    if c.languages.iter().any(|l| l.family == LanguageFamily::OldRealm) {
        let lore = c.ability(AbilityKind::Lore);
        if lore < 1 {
            report.push(ValidationError::OldRealmRequiresLore { lore });
        }
    }

    if c.languages.iter().any(|l| l.family == LanguageFamily::GuildCant) {
        let guild_backing = c
            .backgrounds_of(BackgroundKind::Backing)
            .filter(|b| b.label.eq_ignore_ascii_case("guild"))
            .map(|b| b.trait_.dots())
            .max()
            .unwrap_or(0);
        if guild_backing < 2 {
            report.push(ValidationError::GuildCantRequiresBacking {
                backing: guild_backing,
            });
        }
    }
}
