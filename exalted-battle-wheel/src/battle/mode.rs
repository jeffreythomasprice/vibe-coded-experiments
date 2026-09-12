//! Which of Exalted 2E's three tick-driven combat systems a battle runs (RULES.md §11). All
//! three share the same Speed/DV/refresh machinery; mass combat (p. 158) and social combat
//! (p. 169) only change the scale of a tick — a "long tick" of roughly one minute rather than
//! roughly one second — and which actions are on the menu. Fixed before `Phase::Running`, exactly
//! like the reaction count (see `BattleEvent::SetMode`).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BattleMode {
    #[default]
    Personal,
    Mass,
    Social,
}

impl BattleMode {
    pub const ALL: [BattleMode; 3] = [BattleMode::Personal, BattleMode::Mass, BattleMode::Social];

    pub fn label(self) -> &'static str {
        match self {
            BattleMode::Personal => "Personal combat",
            BattleMode::Mass => "Mass combat",
            BattleMode::Social => "Social combat",
        }
    }

    /// The book's own noun for one increment of this mode's combat time (RULES.md §1.1 p. 141;
    /// §11.1 p. 158; §11.2 p. 169).
    pub fn tick_noun(self) -> &'static str {
        match self {
            BattleMode::Personal => "tick",
            BattleMode::Mass | BattleMode::Social => "long tick",
        }
    }

    pub fn tick_plural(self) -> &'static str {
        match self {
            BattleMode::Personal => "ticks",
            BattleMode::Mass | BattleMode::Social => "long ticks",
        }
    }

    /// A one-line reminder appended to tick-flavored teaching text outside personal combat, via
    /// `DetailTip`. `None` in personal combat, where the existing text already reads correctly.
    pub fn tick_note(self) -> Option<&'static str> {
        match self {
            BattleMode::Personal => None,
            BattleMode::Mass => Some("In mass combat a \u{201c}long tick\u{201d} is roughly one minute, not one second (RULES.md \u{a7}11.1, p. 158)."),
            BattleMode::Social => Some("In social combat a \u{201c}long tick\u{201d} is roughly one minute, not one second (RULES.md \u{a7}11.2, p. 169)."),
        }
    }

    /// What the roll that sets First Action is called in this mode: Join Battle (p. 141), Join
    /// War (p. 162), or Join Debate (p. 169). All three feed the same
    /// `clamp(reaction_count - successes, 0, 6)` schedule (see `JoinBattleResult::speed`) — only
    /// the dice pool behind the successes differs, and the app is given successes, not pools.
    pub fn join_roll_name(self) -> &'static str {
        match self {
            BattleMode::Personal => "Join Battle",
            BattleMode::Mass => "Join War",
            BattleMode::Social => "Join Debate",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_mode_is_personal() {
        assert_eq!(BattleMode::default(), BattleMode::Personal);
    }

    #[test]
    fn only_personal_uses_the_short_tick_noun() {
        assert_eq!(BattleMode::Personal.tick_noun(), "tick");
        assert_eq!(BattleMode::Mass.tick_noun(), "long tick");
        assert_eq!(BattleMode::Social.tick_noun(), "long tick");
    }

    #[test]
    fn only_personal_has_no_tick_note() {
        assert_eq!(BattleMode::Personal.tick_note(), None);
        assert!(BattleMode::Mass.tick_note().is_some());
        assert!(BattleMode::Social.tick_note().is_some());
    }

    #[test]
    fn every_mode_serializes_as_kebab_case() {
        assert_eq!(serde_json::to_string(&BattleMode::Personal).unwrap(), "\"personal\"");
        assert_eq!(serde_json::to_string(&BattleMode::Mass).unwrap(), "\"mass\"");
        assert_eq!(serde_json::to_string(&BattleMode::Social).unwrap(), "\"social\"");
    }
}
