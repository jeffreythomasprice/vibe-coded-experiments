use anyhow::Result;
use rand::prelude::*;

use crate::schema_types::*;
use super::{LlmClient, naming};

pub async fn generate_player(
    rng: &mut impl Rng,
    contexts: &[GeneratorContext],
    config: &Config,
    existing_names: &[String],
    llm: &dyn LlmClient,
) -> Result<Player> {
    let base = 10_i64;
    let mut player = Player {
        name: String::new(),
        description: String::new(),
        stats: PlayerStats {
            strength: base + rng.random_range(-3..=3),
            speed: base + rng.random_range(-3..=3),
            intelligence: base + rng.random_range(-3..=3),
            sanity: base + rng.random_range(-3..=3),
        },
        color: None,
    };

    naming::name_player(&mut player, contexts, config, existing_names, llm).await?;
    Ok(player)
}
