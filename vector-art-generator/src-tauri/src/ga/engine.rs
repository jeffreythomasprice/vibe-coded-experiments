use rand::Rng;

use crate::genome::crossover::crossover;
use crate::genome::distance::genome_distance;
use crate::genome::mutation::mutate;
use crate::genome::random::random_genome;
use crate::genome::types::Genome;

const POPULATION_SIZE: usize = 12;
const ELITISM_COUNT: usize = 2;
const CROSSOVER_RATE: f64 = 0.7;
const IMMIGRANT_COUNT: usize = 1;
const MIN_DIVERSITY_DISTANCE: f64 = 0.05;
const MAX_DIVERSITY_RETRIES: usize = 3;

pub fn breed_next_generation(
    current: &[Genome],
    selected_ids: &[String],
    generation: u32,
) -> Result<Vec<Genome>, String> {
    if selected_ids.is_empty() {
        return Err("Must select at least one organism".into());
    }

    let selected: Vec<&Genome> = current
        .iter()
        .filter(|g| selected_ids.contains(&g.id.to_string()))
        .collect();

    if selected.is_empty() {
        return Err("No matching organisms found for the given IDs".into());
    }

    let mut rng = rand::thread_rng();
    let mut next_gen: Vec<Genome> = Vec::with_capacity(POPULATION_SIZE);

    // Elitism: top selected survive unchanged
    for &elite in selected.iter().take(ELITISM_COUNT) {
        let mut clone = elite.clone();
        clone.generation = generation;
        next_gen.push(clone);
    }

    // Fill remaining slots with bred children
    let children_needed = POPULATION_SIZE - ELITISM_COUNT - IMMIGRANT_COUNT;
    for _ in 0..children_needed {
        let child = breed_diverse_child(&selected, &next_gen, generation, &mut rng);
        next_gen.push(child);
    }

    // Random immigrants for diversity
    for _ in 0..IMMIGRANT_COUNT {
        next_gen.push(random_genome(generation));
    }

    Ok(next_gen)
}

fn breed_diverse_child(
    selected: &[&Genome],
    existing: &[Genome],
    generation: u32,
    rng: &mut impl Rng,
) -> Genome {
    for _ in 0..MAX_DIVERSITY_RETRIES {
        let mut child = breed_child(selected, generation, rng);
        if is_diverse_enough(&child, existing) {
            return child;
        }
        // Too similar — apply extra mutation and retry
        mutate(&mut child);
    }
    // Give up — use a random immigrant instead
    random_genome(generation)
}

fn breed_child(selected: &[&Genome], generation: u32, rng: &mut impl Rng) -> Genome {
    if selected.len() >= 2 && rng.gen_bool(CROSSOVER_RATE) {
        let a = selected[rng.gen_range(0..selected.len())];
        let b = selected[rng.gen_range(0..selected.len())];
        let mut child = crossover(a, b, generation);
        mutate(&mut child);
        child
    } else {
        let parent = selected[rng.gen_range(0..selected.len())];
        let mut child = parent.clone();
        child.id = uuid::Uuid::new_v4();
        child.parent_ids = vec![parent.id];
        child.generation = generation;
        mutate(&mut child);
        child
    }
}

fn is_diverse_enough(candidate: &Genome, population: &[Genome]) -> bool {
    population
        .iter()
        .all(|g| genome_distance(candidate, g) >= MIN_DIVERSITY_DISTANCE)
}
