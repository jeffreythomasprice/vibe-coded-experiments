use rand::Rng;
use uuid::Uuid;

use super::types::*;

pub fn crossover(parent_a: &Genome, parent_b: &Genome, generation: u32) -> Genome {
    let mut rng = rand::thread_rng();
    let a_count = parent_a.tree.node_count();
    let b_count = parent_b.tree.node_count();

    // Try subtree swap up to 10 times, respecting depth limit
    for _ in 0..10 {
        let a_idx = rng.gen_range(0..a_count);
        let b_idx = rng.gen_range(0..b_count);

        if let Some(subtree) = parent_b.tree.node_at(b_idx) {
            let new_tree = parent_a.tree.replace_at(a_idx, subtree.clone());
            if new_tree.depth() <= MAX_DEPTH {
                return Genome {
                    id: Uuid::new_v4(),
                    parent_ids: vec![parent_a.id, parent_b.id],
                    generation,
                    tree: new_tree,
                };
            }
        }
    }

    // Fallback: clone parent A
    Genome {
        id: Uuid::new_v4(),
        parent_ids: vec![parent_a.id],
        generation,
        tree: parent_a.tree.clone(),
    }
}
