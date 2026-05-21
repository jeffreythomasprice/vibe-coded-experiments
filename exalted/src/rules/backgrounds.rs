//! Background-specific validation. The chargen-pool / per-dot caps live in
//! `chargen::check_backgrounds`; this module handles the interlocks between
//! Backgrounds that the rulebook spells out.

use crate::character::{BackgroundKind, Character};
use crate::error::{ValidationError, ValidationReport};

/// Followers requires Resources, Backing **or** Influence at least equal to
/// the Followers rating, in order to support the followers
/// (Exalted 2E p.114). The book's wording is disjunctive — any one of the
/// three suffices — so we compare against the maximum of the three totals.
pub fn validate_backgrounds(c: &Character, report: &mut ValidationReport) {
    let support = c
        .background(BackgroundKind::Resources)
        .max(c.background(BackgroundKind::Backing))
        .max(c.background(BackgroundKind::Influence));

    for followers in c.backgrounds_of(BackgroundKind::Followers) {
        let dots = followers.trait_().dots();
        if dots > support {
            report.push(ValidationError::FollowersWithoutSupport {
                followers: dots,
                support,
            });
        }
    }
}
