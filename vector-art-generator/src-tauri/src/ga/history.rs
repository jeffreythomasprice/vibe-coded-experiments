use serde::{Deserialize, Serialize};
use crate::genome::types::Genome;

#[derive(Serialize, Deserialize)]
pub struct GenerationRecord {
    pub generation: u32,
    pub organism_ids: Vec<String>,
    pub selected_ids: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct History {
    pub records: Vec<GenerationRecord>,
}

impl History {
    pub fn new() -> Self {
        History {
            records: Vec::new(),
        }
    }

    pub fn record(&mut self, generation: u32, organisms: &[Genome], selected_ids: &[String]) {
        self.records.push(GenerationRecord {
            generation,
            organism_ids: organisms.iter().map(|g| g.id.to_string()).collect(),
            selected_ids: selected_ids.to_vec(),
        });
    }
}
