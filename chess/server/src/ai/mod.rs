pub mod manager;
pub mod random;
pub mod registry;

use chess_shared::*;

/// Trait for AI move generation engines.
pub trait AiEngine: Send + Sync {
    /// Unique name for this engine (e.g. "random").
    fn name(&self) -> &str;

    /// Whether this engine supports the given game variant.
    fn supports_variant(&self, variant: &GameVariant) -> bool;

    /// Generate a move for the given position. Returns None if no legal moves exist.
    fn generate_move(
        &self,
        board: &BoardState,
        color: &PieceColor,
        variant: &GameVariant,
        move_history: &[Move],
    ) -> Option<Move>;
}
