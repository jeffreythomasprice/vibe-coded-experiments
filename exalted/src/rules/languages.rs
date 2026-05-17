use std::collections::BTreeSet;

use crate::character::{AbilityKind, BackgroundKind, Character, LanguageFamily};
use crate::error::{ValidationError, ValidationReport};

pub fn validate_languages(c: &Character, report: &mut ValidationReport) {
    let natives: Vec<_> = c.languages.iter().filter(|l| l.native).collect();
    match natives.len() {
        0 => report.push(ValidationError::NoNativeLanguage),
        1 => {
            // p.112: the native tongue comes with a free dialect specialty
            // (the dialect spoken in the character's homeland).
            let native = natives[0];
            if native.dialect_specialty.is_none() {
                report.push(ValidationError::NativeLanguageMissingDialect {
                    family: format!("{:?}", native.family),
                });
            }
        }
        n => report.push(ValidationError::MultipleNativeLanguages { got: n }),
    }

    let linguistics = c.ability(AbilityKind::Linguistics);

    // p.111-112: 1 native family + 1 family per Linguistics dot. Tribal
    // tongues don't count against this cap.
    let max_families = 1 + linguistics as usize;
    let non_tribal_count = c.languages.iter().filter(|l| !l.family.is_tribal()).count();
    if non_tribal_count > max_families {
        report.push(ValidationError::TooManyLanguages {
            got: non_tribal_count,
            max: max_families,
        });
    }

    // p.112: each Linguistics dot also grants 4 tribal tongues.
    let max_tribal = (linguistics as usize).saturating_mul(4);
    let tribal_count = c.languages.iter().filter(|l| l.family.is_tribal()).count();
    if tribal_count > max_tribal {
        report.push(ValidationError::TooManyTribalTongues {
            got: tribal_count,
            max: max_tribal,
        });
    }

    let mut seen: BTreeSet<LanguageFamily> = BTreeSet::new();
    for lang in &c.languages {
        if !seen.insert(lang.family.clone()) {
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
