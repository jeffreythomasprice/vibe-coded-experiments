use std::collections::HashMap;

use chess_shared::*;
use rand::Rng;

use super::AiEngine;

/// AI engine that picks a random legal move.
pub struct RandomEngine;

impl AiEngine for RandomEngine {
    fn name(&self) -> &str {
        "random"
    }

    fn description(&self) -> &str {
        "Picks a random legal move"
    }

    fn supports_variant(&self, _variant: &GameVariant) -> bool {
        true
    }

    fn generate_move(
        &self,
        board: &BoardState,
        color: &PieceColor,
        variant: &GameVariant,
        move_history: &[Move],
        _params: &HashMap<String, String>,
    ) -> Option<Move> {
        let moves = chess_engine::legal_moves(board, color, variant, move_history);
        if moves.is_empty() {
            return None;
        }

        // Prefer moves that don't lead to repeated positions
        let position_keys = chess_engine::build_position_keys(move_history);
        let non_repeating: Vec<&Move> = moves
            .iter()
            .filter(|m| !chess_engine::would_repeat(board, m, color, move_history, &position_keys))
            .collect();

        let mut rng = rand::thread_rng();
        if non_repeating.is_empty() {
            let idx = rng.gen_range(0..moves.len());
            Some(moves.into_iter().nth(idx).unwrap())
        } else {
            let idx = rng.gen_range(0..non_repeating.len());
            Some(non_repeating[idx].clone())
        }
    }
}
