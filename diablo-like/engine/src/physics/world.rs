use crate::geom::parts::Level;

use super::body::Actor;
use super::collide::{self, Contact};

/// Owns the single actor this milestone drives with WASD, and the last
/// tick's contacts/iteration count for debug visualization and the HUD's
/// "max trace iterations per tick" counter.
pub struct World {
    pub actor: Actor,
    pub contacts: Vec<Contact>,
    pub max_trace_iterations: u32,
}

impl World {
    pub fn new(actor: Actor) -> Self {
        Self {
            actor,
            contacts: Vec::new(),
            max_trace_iterations: 0,
        }
    }

    pub fn step(&mut self, level: &Level, dt: f32) {
        let stats = collide::move_actor(level, &mut self.actor, dt);
        self.contacts = stats.contacts;
        self.max_trace_iterations = stats.max_trace_iterations;
    }
}
