mod effect;
mod event;
mod item;
pub mod naming;
mod player;
mod room;

pub use effect::generate_effect;
pub use event::generate_event;
pub use item::generate_item;
pub use naming::OllamaClient;
pub use player::generate_player;
pub use room::generate_room;

use anyhow::Result;
use async_trait::async_trait;
use rand::distr::weighted::WeightedIndex;
use rand::prelude::*;

#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn complete(&self, model: &str, prompt: &str) -> Result<String>;
}

pub fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

pub struct WeightTable<T: Clone> {
    variants: Vec<T>,
    weights_low: Vec<f64>,
    weights_high: Vec<f64>,
}

impl<T: Clone> WeightTable<T> {
    pub fn new(entries: &[(T, f64, f64)]) -> Self {
        let mut variants = Vec::with_capacity(entries.len());
        let mut weights_low = Vec::with_capacity(entries.len());
        let mut weights_high = Vec::with_capacity(entries.len());
        for (v, lo, hi) in entries {
            variants.push(v.clone());
            weights_low.push(*lo);
            weights_high.push(*hi);
        }
        Self { variants, weights_low, weights_high }
    }

    pub fn sample(&self, magnitude: f64, rng: &mut impl Rng) -> T {
        let weights: Vec<f64> = self
            .weights_low
            .iter()
            .zip(&self.weights_high)
            .map(|(lo, hi)| lerp(*lo, *hi, magnitude))
            .collect();
        let dist = WeightedIndex::new(&weights).expect("invalid weights");
        self.variants[dist.sample(rng)].clone()
    }
}

pub fn roll_effect_count(magnitude: f64, rng: &mut impl Rng) -> usize {
    let table = WeightTable::new(&[
        (1_usize, 1.0, 0.1),
        (2, 0.0, 0.5),
        (3, 0.0, 0.4),
    ]);
    table.sample(magnitude, rng)
}

const STAT_NAMES: &[&str] = &[
    "strength", "speed", "intelligence", "sanity",
];

pub fn random_stat(rng: &mut impl Rng) -> String {
    STAT_NAMES.choose(rng).unwrap().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    #[test]
    fn lerp_edges() {
        assert_eq!(lerp(0.0, 10.0, 0.0), 0.0);
        assert_eq!(lerp(0.0, 10.0, 1.0), 10.0);
        assert_eq!(lerp(0.0, 10.0, 0.5), 5.0);
        assert_eq!(lerp(-5.0, 5.0, 0.5), 0.0);
    }

    #[test]
    fn weight_table_low_magnitude_favors_first() {
        let table = WeightTable::new(&[
            ("a", 10.0, 0.0),
            ("b", 0.0, 10.0),
        ]);
        let mut rng = StdRng::seed_from_u64(42);
        let mut a_count = 0;
        for _ in 0..100 {
            if table.sample(0.0, &mut rng) == "a" {
                a_count += 1;
            }
        }
        assert_eq!(a_count, 100);
    }

    #[test]
    fn weight_table_high_magnitude_favors_second() {
        let table = WeightTable::new(&[
            ("a", 10.0, 0.0),
            ("b", 0.0, 10.0),
        ]);
        let mut rng = StdRng::seed_from_u64(42);
        let mut b_count = 0;
        for _ in 0..100 {
            if table.sample(1.0, &mut rng) == "b" {
                b_count += 1;
            }
        }
        assert_eq!(b_count, 100);
    }

    #[test]
    fn roll_effect_count_low_magnitude() {
        let mut rng = StdRng::seed_from_u64(0);
        let counts: Vec<usize> = (0..50).map(|_| roll_effect_count(0.0, &mut rng)).collect();
        assert!(counts.iter().all(|&c| c == 1));
    }

    #[test]
    fn roll_effect_count_high_magnitude() {
        let mut rng = StdRng::seed_from_u64(0);
        let counts: Vec<usize> = (0..100).map(|_| roll_effect_count(1.0, &mut rng)).collect();
        assert!(counts.iter().any(|&c| c >= 2));
    }
}
