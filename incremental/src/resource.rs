use rand::Rng;
use rand::seq::IndexedRandom;

use crate::config::game::{
    COUNT_WEIGHTS, INITIAL_MAX_GENERATE, INITIAL_MAX_OVERFILL, INITIAL_PER_CLICK,
};
use crate::config::theme::Color;

pub const RESOURCE_COUNT: usize = 5;

pub type Amount = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resource {
    Wood,
    Fire,
    Earth,
    Metal,
    Water,
}

impl Resource {
    pub const ALL: [Resource; RESOURCE_COUNT] = [
        Resource::Wood,
        Resource::Fire,
        Resource::Earth,
        Resource::Metal,
        Resource::Water,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn color(self) -> Color {
        match self {
            Resource::Wood => Color::hex(0x4caf50),
            Resource::Fire => Color::hex(0xe53935),
            Resource::Earth => Color::hex(0xa1792f),
            Resource::Metal => Color::hex(0x9aa5b1),
            Resource::Water => Color::hex(0x29b6f6),
        }
    }

    pub fn accent(self) -> Color {
        match self {
            Resource::Wood => Color::hex(0xc5e8a8),
            Resource::Fire => Color::hex(0xffb74d),
            Resource::Earth => Color::hex(0xe0c58a),
            Resource::Metal => Color::hex(0xeef2f6),
            Resource::Water => Color::hex(0xe1f5fe),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceState {
    pub current: Amount,
    pub max_generate: Amount,
    pub max_overfill: Amount,
    pub per_click: Amount,
}

impl Default for ResourceState {
    fn default() -> Self {
        Self {
            current: 0,
            max_generate: INITIAL_MAX_GENERATE,
            max_overfill: INITIAL_MAX_OVERFILL,
            per_click: INITIAL_PER_CLICK,
        }
    }
}

fn ratio(part: Amount, whole: Amount) -> f64 {
    part.min(whole) as f64 / whole as f64
}

impl ResourceState {
    pub fn gather(&mut self) -> Amount {
        if self.current >= self.max_generate {
            return 0;
        }
        let before = self.current;
        self.current = before.saturating_add(self.per_click).min(self.max_generate);
        self.current - before
    }

    pub fn capacity(&self) -> Amount {
        self.max_generate.saturating_add(self.max_overfill)
    }

    pub fn generate_fill(&self) -> f64 {
        if self.max_generate == 0 {
            return 1.0;
        }
        ratio(self.current, self.max_generate)
    }

    pub fn overfill_fill(&self) -> f64 {
        if self.max_overfill == 0 {
            return 0.0;
        }
        ratio(
            self.current.saturating_sub(self.max_generate),
            self.max_overfill,
        )
    }
}

pub fn random_set(rng: &mut impl Rng) -> Vec<Resource> {
    let total: f64 = COUNT_WEIGHTS.iter().sum();
    let mut roll = rng.random::<f64>() * total;
    let mut count = RESOURCE_COUNT;
    for (i, weight) in COUNT_WEIGHTS.iter().enumerate() {
        if roll < *weight {
            count = i + 1;
            break;
        }
        roll -= weight;
    }
    Resource::ALL.choose_multiple(rng, count).copied().collect()
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;
    use rand_pcg::Pcg64Mcg;

    use super::*;

    #[test]
    fn gather_accumulates_up_to_cap() {
        let mut s = ResourceState {
            per_click: 4,
            ..ResourceState::default()
        };
        s.gather();
        assert_eq!(s.current, 4);
        s.gather();
        s.gather();
        assert_eq!(s.current, s.max_generate);
    }

    #[test]
    fn gather_reports_actual_gain() {
        let mut s = ResourceState {
            current: 8,
            per_click: 4,
            ..ResourceState::default()
        };
        assert_eq!(s.gather(), 2);
        assert_eq!(s.gather(), 0);
        let mut s = ResourceState::default();
        assert_eq!(s.gather(), 1);
    }

    #[test]
    fn gather_does_not_reduce_values_above_cap() {
        let mut s = ResourceState {
            current: 50,
            ..ResourceState::default()
        };
        s.gather();
        assert_eq!(s.current, 50);
    }

    #[test]
    fn gather_saturates_instead_of_overflowing() {
        let mut s = ResourceState {
            current: 1,
            max_generate: u64::MAX,
            per_click: u64::MAX,
            ..ResourceState::default()
        };
        s.gather();
        assert_eq!(s.current, u64::MAX);
    }

    #[test]
    fn capacity_is_generate_plus_overfill() {
        assert_eq!(ResourceState::default().capacity(), 100);
    }

    #[test]
    fn fills_track_current() {
        let at = |current| ResourceState {
            current,
            ..ResourceState::default()
        };
        assert_eq!((at(5).generate_fill(), at(5).overfill_fill()), (0.5, 0.0));
        assert_eq!((at(10).generate_fill(), at(10).overfill_fill()), (1.0, 0.0));
        assert_eq!((at(55).generate_fill(), at(55).overfill_fill()), (1.0, 0.5));
        assert_eq!(
            (at(100).generate_fill(), at(100).overfill_fill()),
            (1.0, 1.0)
        );
    }

    #[test]
    fn fills_handle_zero_maximums() {
        let s = ResourceState {
            current: 3,
            max_generate: 0,
            max_overfill: 6,
            per_click: 1,
        };
        assert_eq!((s.generate_fill(), s.overfill_fill()), (1.0, 0.5));
        let s = ResourceState {
            max_generate: 4,
            max_overfill: 0,
            current: 4,
            ..s
        };
        assert_eq!((s.generate_fill(), s.overfill_fill()), (1.0, 0.0));
    }

    #[test]
    fn random_sets_are_distinct_and_non_empty() {
        let mut rng = Pcg64Mcg::seed_from_u64(1);
        for _ in 0..1000 {
            let set = random_set(&mut rng);
            assert!((1..=RESOURCE_COUNT).contains(&set.len()));
            for (i, r) in set.iter().enumerate() {
                assert!(!set[i + 1..].contains(r));
            }
        }
    }

    #[test]
    fn random_set_sizes_follow_weights() {
        let mut rng = Pcg64Mcg::seed_from_u64(2);
        let samples = 20_000;
        let mut counts = [0usize; RESOURCE_COUNT];
        for _ in 0..samples {
            counts[random_set(&mut rng).len() - 1] += 1;
        }
        let single = counts[0] as f64 / samples as f64;
        let double = counts[1] as f64 / samples as f64;
        assert!((single - 0.85).abs() < 0.02, "single: {single}");
        assert!((double - 0.10).abs() < 0.02, "double: {double}");
    }

    #[test]
    fn every_resource_is_reachable() {
        let mut rng = Pcg64Mcg::seed_from_u64(3);
        let mut seen = [false; RESOURCE_COUNT];
        for _ in 0..200 {
            for r in random_set(&mut rng) {
                seen[r.index()] = true;
            }
        }
        assert!(seen.iter().all(|&s| s));
    }
}
