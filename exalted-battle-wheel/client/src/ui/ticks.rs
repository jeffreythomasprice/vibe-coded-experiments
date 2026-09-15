//! Mode-aware phrasing for tick numbers (RULES.md §11), so no call site branches on `BattleMode`
//! to say "long tick". The bare nouns live on `BattleMode` itself — they're the book's own
//! vocabulary — and only the composed phrases live here, keeping presentation strings out of
//! `battle`. Collapses three near-duplicate span formatters that used to live separately in
//! `event_log.rs`, `queue.rs`, and `wheel.rs`.

use shared::battle::{BattleMode, Tick};

/// "tick 7" / "long tick 7"
pub fn at(mode: BattleMode, tick: Tick) -> String {
    format!("{} {tick}", mode.tick_noun())
}

/// "tick 5" / "long ticks 8–10"
pub fn span(mode: BattleMode, at_tick: Tick, ticks: u32) -> String {
    if ticks <= 1 {
        at(mode, at_tick)
    } else {
        format!("{} {at_tick}\u{2013}{}", mode.tick_plural(), at_tick + ticks - 1)
    }
}

/// "1 tick" / "3 long ticks"
pub fn count(mode: BattleMode, ticks: u32) -> String {
    let noun = if ticks == 1 { mode.tick_noun() } else { mode.tick_plural() };
    format!("{ticks} {noun}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn personal_at_uses_the_bare_tick_noun() {
        assert_eq!(at(BattleMode::Personal, 7), "tick 7");
        assert_eq!(at(BattleMode::Mass, 7), "long tick 7");
    }

    #[test]
    fn span_of_one_tick_falls_back_to_at() {
        assert_eq!(span(BattleMode::Personal, 5, 1), "tick 5");
    }

    #[test]
    fn span_of_several_ticks_uses_the_plural_and_a_range() {
        assert_eq!(span(BattleMode::Personal, 8, 3), "ticks 8\u{2013}10");
        assert_eq!(span(BattleMode::Social, 8, 3), "long ticks 8\u{2013}10");
    }

    #[test]
    fn count_pluralizes_correctly() {
        assert_eq!(count(BattleMode::Personal, 1), "1 tick");
        assert_eq!(count(BattleMode::Personal, 3), "3 ticks");
        assert_eq!(count(BattleMode::Mass, 1), "1 long tick");
    }
}
