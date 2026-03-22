use crate::generated::RenderedOrganism;
use crate::genome::random::random_genome;
use crate::genome::render::render_genome;
use crate::genome::types::Genome;
use crate::ga::engine;
use crate::ga::history::History;
use serde::{Deserialize, Serialize};

const INITIAL_POPULATION_SIZE: usize = 12;

#[derive(Serialize, Deserialize)]
pub struct Population {
    name: String,
    generation: u32,
    organisms: Vec<Genome>,
    history: History,
}

impl Population {
    pub fn new(name: &str) -> Self {
        let organisms: Vec<Genome> = (0..INITIAL_POPULATION_SIZE)
            .map(|_| random_genome(0))
            .collect();

        let mut history = History::new();
        history.record(0, &organisms, &[]);

        Population {
            name: name.to_string(),
            generation: 0,
            organisms,
            history,
        }
    }

    pub fn render_all(&self) -> Vec<RenderedOrganism> {
        self.organisms
            .iter()
            .map(|g| RenderedOrganism {
                id: g.id.to_string(),
                svg: render_genome(g),
                generation: self.generation as u64,
            })
            .collect()
    }

    pub fn breed(&mut self, selected_ids: &[String]) -> Result<(), String> {
        self.generation += 1;
        let next = engine::breed_next_generation(&self.organisms, selected_ids, self.generation)?;
        self.history.record(self.generation, &next, selected_ids);
        self.organisms = next;
        Ok(())
    }

}

impl Default for Population {
    fn default() -> Self {
        Self::new("default")
    }
}
