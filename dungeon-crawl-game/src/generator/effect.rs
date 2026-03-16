use rand::prelude::*;

use crate::schema_types::*;
use super::{WeightTable, lerp, random_stat};

fn result_type_table() -> WeightTable<EffectResultType> {
    use EffectResultType::*;
    WeightTable::new(&[
        (ModifyStat,      30.0, 10.0),
        (PhysicalDamage,  30.0, 10.0),
        (MentalDamage,    30.0, 10.0),
        (Teleport,         1.0, 10.0),
        (LoseTurn,         1.0, 10.0),
    ])
}

fn trigger_type_table() -> WeightTable<EffectTriggerType> {
    use EffectTriggerType::*;
    WeightTable::new(&[
        (Always,     0.8, 0.2),
        (StatCheck,  0.1, 0.4),
        (RandomRoll, 0.1, 0.4),
    ])
}

fn target_table() -> WeightTable<EffectTarget> {
    use EffectTarget::*;
    WeightTable::new(&[
        (SelfKind, 0.8, 0.3),
        (Room,     0.1, 0.35),
        (All,      0.1, 0.35),
    ])
}

fn amount_table() -> WeightTable<i64> {
    WeightTable::new(&[
        (1, 100.0, 20.0),
        (2,  10.0, 15.0),
        (3,   1.0, 10.0),
        (4,   0.1,  5.0),
        (5,  0.01,  2.0),
    ])
}

fn timing_type_table() -> WeightTable<EffectTimingType> {
    use EffectTimingType::*;
    WeightTable::new(&[
        (Immediate, 0.8, 0.3),
        (Duration,  0.1, 0.35),
        (Delayed,   0.1, 0.35),
    ])
}

pub fn generate_effect(magnitude: f64, rng: &mut impl Rng) -> Effect {
    let result_type = result_type_table().sample(magnitude, rng);
    let trigger_type = trigger_type_table().sample(magnitude, rng);
    let target = target_table().sample(magnitude, rng);
    let timing_type = timing_type_table().sample(magnitude, rng);

    let mut result_stat = None;
    let mut result_amount = None;

    match &result_type {
        EffectResultType::ModifyStat => {
            result_stat = Some(random_stat(rng));
            result_amount = Some(amount_table().sample(magnitude, rng));
        }
        EffectResultType::PhysicalDamage | EffectResultType::MentalDamage => {
            result_amount = Some(amount_table().sample(magnitude, rng));
        }
        EffectResultType::Teleport | EffectResultType::LoseTurn => {}
    }

    let trigger_stat = match &trigger_type {
        EffectTriggerType::StatCheck => Some(random_stat(rng)),
        EffectTriggerType::Always | EffectTriggerType::RandomRoll => None,
    };

    let timing_turns = match &timing_type {
        EffectTimingType::Duration | EffectTimingType::Delayed => {
            Some(lerp(1.0, 5.0, magnitude).round() as i64)
        }
        EffectTimingType::Immediate => None,
    };

    Effect {
        trigger_type,
        result_type,
        target,
        trigger_stat,
        trigger_outcomes: None,
        result_stat,
        result_amount,
        timing_type,
        timing_turns,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    #[test]
    fn generate_effect_low_magnitude() {
        let mut rng = StdRng::seed_from_u64(42);
        let effect = generate_effect(0.0, &mut rng);
        if let Some(amt) = effect.result_amount {
            assert!(amt >= 1 && amt <= 5);
        }
    }

    #[test]
    fn generate_effect_high_magnitude() {
        let mut rng = StdRng::seed_from_u64(42);
        let effect = generate_effect(1.0, &mut rng);
        if let Some(amt) = effect.result_amount {
            assert!(amt >= 1 && amt <= 5);
        }
    }

    #[test]
    fn generate_effect_has_trigger_stat_for_stat_check() {
        let mut rng = StdRng::seed_from_u64(0);
        for _ in 0..200 {
            let effect = generate_effect(1.0, &mut rng);
            if matches!(effect.trigger_type, EffectTriggerType::StatCheck) {
                assert!(effect.trigger_stat.is_some());
            }
        }
    }

    #[test]
    fn generate_effect_timing_turns_for_duration() {
        let mut rng = StdRng::seed_from_u64(0);
        for _ in 0..200 {
            let effect = generate_effect(1.0, &mut rng);
            match effect.timing_type {
                EffectTimingType::Duration | EffectTimingType::Delayed => {
                    assert!(effect.timing_turns.is_some());
                }
                EffectTimingType::Immediate => {
                    assert!(effect.timing_turns.is_none());
                }
            }
        }
    }

    #[test]
    fn generate_effect_modify_stat_has_stat() {
        let mut rng = StdRng::seed_from_u64(0);
        for _ in 0..200 {
            let effect = generate_effect(0.5, &mut rng);
            if matches!(effect.result_type, EffectResultType::ModifyStat) {
                assert!(effect.result_stat.is_some());
                assert!(effect.result_amount.is_some());
            }
        }
    }
}
