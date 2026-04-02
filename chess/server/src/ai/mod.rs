pub mod manager;
pub mod minimax;
pub mod random;
pub mod registry;

use std::collections::HashMap;

use chess_shared::*;

/// Trait for AI move generation engines.
pub trait AiEngine: Send + Sync {
    /// Unique name for this engine (e.g. "random").
    fn name(&self) -> &str;

    /// Human-readable description of this engine.
    fn description(&self) -> &str;

    /// Whether this engine supports the given game variant.
    fn supports_variant(&self, variant: &GameVariant) -> bool;

    /// Parameter definitions for this engine. Defaults to no parameters.
    fn parameter_defs(&self) -> Vec<AiParameterDef> {
        vec![]
    }

    /// Generate a move for the given position. Returns None if no legal moves exist.
    fn generate_move(
        &self,
        board: &BoardState,
        color: &PieceColor,
        variant: &GameVariant,
        move_history: &[Move],
        params: &HashMap<String, String>,
    ) -> Option<Move>;
}
