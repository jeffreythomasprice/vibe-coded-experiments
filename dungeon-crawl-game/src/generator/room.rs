use anyhow::Result;
use rand::prelude::*;

use crate::schema_types::*;
use super::{generate_event, generate_item, naming};

pub async fn generate_room(
    magnitude: f64,
    rng: &mut impl Rng,
    contexts: &[GeneratorContext],
    config: &Config,
) -> Result<Room> {
    let arrangements = [
        DoorConfigArrangement::DeadEnd,
        DoorConfigArrangement::Straight,
        DoorConfigArrangement::Corner,
        DoorConfigArrangement::TIntersection,
        DoorConfigArrangement::Crossroads,
    ];
    let arrangement = arrangements.choose(rng).unwrap().clone();

    let content = if rng.random_bool(0.5) {
        RoomContent::Item(generate_item(magnitude, rng, contexts, config).await?)
    } else {
        RoomContent::Event(generate_event(magnitude, rng, contexts, config).await?)
    };

    let mut room = Room {
        name: String::new(),
        description: String::new(),
        door_config: DoorConfig { arrangement },
        magnitude,
        content: Some(content),
    };

    naming::name_room(&mut room, contexts, config).await?;
    Ok(room)
}
