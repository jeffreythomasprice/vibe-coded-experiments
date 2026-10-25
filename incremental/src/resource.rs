use rand::Rng;
use rand::seq::IndexedRandom;

pub const RESOURCE_COUNT: usize = 5;

const COUNT_WEIGHTS: [f64; RESOURCE_COUNT] = [85.0, 10.0, 4.0, 0.9, 0.1];

const INITIAL_MAX_GENERATE: f64 = 10.0;
const INITIAL_MAX_OVERFILL: f64 = 90.0;
const INITIAL_PER_SECOND: f64 = 1.0;

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

    pub fn color(self) -> &'static str {
        match self {
            Resource::Wood => "#4caf50",
            Resource::Fire => "#e53935",
            Resource::Earth => "#a1792f",
            Resource::Metal => "#9aa5b1",
            Resource::Water => "#29b6f6",
        }
    }

    pub fn accent(self) -> &'static str {
        match self {
            Resource::Wood => "#c5e8a8",
            Resource::Fire => "#ffb74d",
            Resource::Earth => "#e0c58a",
            Resource::Metal => "#eef2f6",
            Resource::Water => "#e1f5fe",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResourceState {
    pub current: f64,
    pub max_generate: f64,
    pub max_overfill: f64,
    pub per_second: f64,
}

impl Default for ResourceState {
    fn default() -> Self {
        Self {
            current: 0.0,
            max_generate: INITIAL_MAX_GENERATE,
            max_overfill: INITIAL_MAX_OVERFILL,
            per_second: INITIAL_PER_SECOND,
        }
    }
}

impl ResourceState {
    pub fn gather(&mut self, dt: f64) {
        if self.current < self.max_generate {
            self.current = (self.current + self.per_second * dt).min(self.max_generate);
        }
    }

    pub fn capacity(&self) -> f64 {
        self.max_generate + self.max_overfill
    }

    pub fn generate_fill(&self) -> f64 {
        if self.max_generate <= 0.0 {
            return 1.0;
        }
        (self.current / self.max_generate).clamp(0.0, 1.0)
    }

    pub fn overfill_fill(&self) -> f64 {
        if self.max_overfill <= 0.0 {
            return 0.0;
        }
        ((self.current - self.max_generate) / self.max_overfill).clamp(0.0, 1.0)
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
        let mut s = ResourceState::default();
        s.gather(4.0);
        assert_eq!(s.current, 4.0);
        s.gather(100.0);
        assert_eq!(s.current, s.max_generate);
    }

    #[test]
    fn gather_does_not_reduce_values_above_cap() {
        let mut s = ResourceState {
            current: 50.0,
            ..ResourceState::default()
        };
        s.gather(1.0);
        assert_eq!(s.current, 50.0);
    }

    #[test]
    fn capacity_is_generate_plus_overfill() {
        assert_eq!(ResourceState::default().capacity(), 100.0);
    }

    #[test]
    fn fills_track_current() {
        let at = |current| ResourceState {
            current,
            ..ResourceState::default()
        };
        assert_eq!(
            (at(5.0).generate_fill(), at(5.0).overfill_fill()),
            (0.5, 0.0)
        );
        assert_eq!(
            (at(10.0).generate_fill(), at(10.0).overfill_fill()),
            (1.0, 0.0)
        );
        assert_eq!(
            (at(55.0).generate_fill(), at(55.0).overfill_fill()),
            (1.0, 0.5)
        );
        assert_eq!(
            (at(100.0).generate_fill(), at(100.0).overfill_fill()),
            (1.0, 1.0)
        );
    }

    #[test]
    fn fills_handle_zero_maximums() {
        let s = ResourceState {
            current: 3.0,
            max_generate: 0.0,
            max_overfill: 6.0,
            per_second: 1.0,
        };
        assert_eq!((s.generate_fill(), s.overfill_fill()), (1.0, 0.5));
        let s = ResourceState {
            max_generate: 4.0,
            max_overfill: 0.0,
            current: 4.0,
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
