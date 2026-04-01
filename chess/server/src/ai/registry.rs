use std::collections::HashMap;
use std::sync::Arc;

use chess_shared::GameVariant;

use super::random::RandomEngine;
use super::AiEngine;

/// Registry of available AI engines.
pub struct AiRegistry {
    engines: HashMap<String, Arc<dyn AiEngine>>,
}

impl AiRegistry {
    pub fn new() -> Self {
        let mut engines: HashMap<String, Arc<dyn AiEngine>> = HashMap::new();
        engines.insert("random".to_string(), Arc::new(RandomEngine));
        Self { engines }
    }

    /// Get an engine by name.
    pub fn get(&self, name: &str) -> Option<Arc<dyn AiEngine>> {
        self.engines.get(name).cloned()
    }

    /// List engines that support the given variant.
    pub fn available_for_variant(&self, variant: &GameVariant) -> Vec<String> {
        self.engines
            .iter()
            .filter(|(_, engine)| engine.supports_variant(variant))
            .map(|(name, _)| name.clone())
            .collect()
    }
}
