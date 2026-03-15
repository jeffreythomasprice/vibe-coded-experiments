use anyhow::Result;
use rand::prelude::*;

use crate::schema_types::*;
use super::naming;

pub async fn generate_player(
    rng: &mut impl Rng,
    contexts: &[GeneratorContext],
    config: &Config,
) -> Result<Player> {
    let base = 10_i64;
    let mut player = Player {
        name: String::new(),
        description: String::new(),
        stats: PlayerStats {
            strength: base + rng.random_range(-3..=3),
            dexterity: base + rng.random_range(-3..=3),
            intelligence: base + rng.random_range(-3..=3),
            health: base + rng.random_range(-3..=3),
            speed: base + rng.random_range(-3..=3),
            sanity: base + rng.random_range(-3..=3),
        },
        color: None,
    };

    naming::name_player(&mut player, contexts, config).await?;
    Ok(player)
}
