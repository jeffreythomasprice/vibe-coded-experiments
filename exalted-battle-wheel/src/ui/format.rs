//! Shared display formatting for action data, used by both the action panel's summary chips and
//! the reference rail so the two never drift apart.

use exalted_battle_wheel::battle::{DvPenaltySpec, SpeedSpec};

pub fn format_speed(spec: SpeedSpec) -> String {
    match spec {
        SpeedSpec::Fixed(speed) => speed.to_string(),
        SpeedSpec::Variable { default } => format!("varies (default {default})"),
    }
}

pub fn format_dv_penalty(spec: DvPenaltySpec) -> String {
    match spec {
        DvPenaltySpec::Fixed(penalty) => penalty.to_string(),
        DvPenaltySpec::Variable { default } => format!("varies (default {default})"),
    }
}

/// Compact forms for tight spaces like the reference rail: "5" stays "5", a variable Speed/DV
/// reads as its default with a trailing asterisk rather than the full "varies (default N)".
pub fn format_speed_compact(spec: SpeedSpec) -> String {
    match spec {
        SpeedSpec::Fixed(speed) => speed.to_string(),
        SpeedSpec::Variable { default } => format!("{default}*"),
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
        assert_eq!(format_speed(SpeedSpec::Fixed(3)), "3");
    }

    #[test]
    fn variable_speed_names_its_default() {
        assert_eq!(format_speed(SpeedSpec::Variable { default: 5 }), "varies (default 5)");
    }

    #[test]
    fn compact_variable_speed_is_starred() {
        assert_eq!(format_speed_compact(SpeedSpec::Variable { default: 5 }), "5*");
    }

    #[test]
    fn compact_fixed_dv_is_bare() {
        assert_eq!(format_dv_penalty_compact(DvPenaltySpec::Fixed(-2)), "-2");
    }
}
