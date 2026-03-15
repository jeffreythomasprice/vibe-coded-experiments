use anyhow::Result;
use rand::prelude::*;

use crate::schema_types::*;
use super::{generate_effect, lerp, naming, roll_effect_count};

pub async fn generate_item(
    magnitude: f64,
    rng: &mut impl Rng,
    contexts: &[GeneratorContext],
    config: &Config,
) -> Result<Item> {
    let count = roll_effect_count(magnitude, rng);
    let effects: Vec<Effect> = (0..count)
        .map(|_| generate_effect(magnitude, rng))
        .collect();

    let cursed = rng.random_bool(lerp(0.0, 0.4, magnitude));

    let mut item = Item {
        name: String::new(),
        description: String::new(),
        effects,
        magnitude,
        droppable: !cursed,
        tradable: !cursed,
    };

    naming::name_item(&mut item, contexts, config).await?;
    Ok(item)
}
