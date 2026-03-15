use anyhow::Result;
use rand::prelude::*;

use super::{generate_effect, naming, roll_effect_count};
use crate::schema_types::*;

pub async fn generate_event(
    magnitude: f64,
    rng: &mut impl Rng,
    contexts: &[GeneratorContext],
    config: &Config,
) -> Result<Event> {
    let count = roll_effect_count(magnitude, rng);
    let effects: Vec<Effect> = (0..count)
        .map(|_| generate_effect(magnitude, rng))
        .collect();

    let mut event = Event {
        name: String::new(),
        description: String::new(),
        effects,
        magnitude,
    };

    naming::name_event(&mut event, contexts, config).await?;
    Ok(event)
}
