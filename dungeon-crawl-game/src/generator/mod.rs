mod effect;
mod event;
mod item;
pub mod naming;
mod player;
mod room;

pub use effect::generate_effect;
pub use event::generate_event;
pub use item::generate_item;
pub use player::generate_player;
pub use room::generate_room;

use rand::distr::weighted::WeightedIndex;
use rand::prelude::*;

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
    "strength", "dexterity", "intelligence", "health", "speed", "sanity",
];

pub fn random_stat(rng: &mut impl Rng) -> String {
    STAT_NAMES.choose(rng).unwrap().to_string()
}
