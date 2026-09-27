//! Shared display formatting for action data, used by both the action panel's summary chips and
//! the reference rail so the two never drift apart.

use shared::battle::{ActionSpeed, DvPenaltySpec};

pub fn format_speed(spec: ActionSpeed) -> String {
    match spec {
        ActionSpeed::Fixed(speed) => speed.to_string(),
        ActionSpeed::Variable { default } => format!("varies (default {default})"),
        ActionSpeed::Required => "varies (no default)".to_string(),
    }
}

pub fn format_dv_penalty(spec: DvPenaltySpec) -> String {
    match spec {
        DvPenaltySpec::Fixed(penalty) => penalty.to_string(),
        DvPenaltySpec::Variable { default } => format!("varies (default {default})"),
    }
}

/// Compact forms for tight spaces like the reference rail: "5" stays "5", a variable Speed/DV
/// reads as its default with a trailing asterisk rather than the full "varies (default N)". A
/// Speed with no default at all (Attack, Social Attack) shows as "?" instead.
pub fn format_speed_compact(spec: ActionSpeed) -> String {
    match spec {
        ActionSpeed::Fixed(speed) => speed.to_string(),
        ActionSpeed::Variable { default } => format!("{default}*"),
        ActionSpeed::Required => "?".to_string(),
    }
}

pub fn format_dv_penalty_compact(spec: DvPenaltySpec) -> String {
    match spec {
        DvPenaltySpec::Fixed(penalty) => penalty.to_string(),
        DvPenaltySpec::Variable { default } => format!("{default}*"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_speed_formats_as_a_bare_number() {
        assert_eq!(format_speed(ActionSpeed::Fixed(3)), "3");
    }

    #[test]
    fn variable_speed_names_its_default() {
        assert_eq!(format_speed(ActionSpeed::Variable { default: 5 }), "varies (default 5)");
    }

    #[test]
    fn required_speed_names_that_it_has_no_default() {
        assert_eq!(format_speed(ActionSpeed::Required), "varies (no default)");
    }

    #[test]
    fn compact_variable_speed_is_starred() {
        assert_eq!(format_speed_compact(ActionSpeed::Variable { default: 5 }), "5*");
    }

    #[test]
    fn compact_required_speed_is_a_question_mark() {
        assert_eq!(format_speed_compact(ActionSpeed::Required), "?");
    }

    #[test]
    fn compact_fixed_dv_is_bare() {
        assert_eq!(format_dv_penalty_compact(DvPenaltySpec::Fixed(-2)), "-2");
    }
}
