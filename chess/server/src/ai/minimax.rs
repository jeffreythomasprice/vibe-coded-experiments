use std::collections::HashMap;

use chess_shared::*;

use super::AiEngine;

/// AI engine using minimax search with alpha-beta pruning.
pub struct MinimaxEngine;

impl AiEngine for MinimaxEngine {
    fn name(&self) -> &str {
        "minimax"
    }

    fn description(&self) -> &str {
        "Minimax search with alpha-beta pruning"
    }

    fn supports_variant(&self, _variant: &GameVariant) -> bool {
        true
    }

    fn parameter_defs(&self) -> Vec<AiParameterDef> {
        vec![AiParameterDef {
            name: "depth".into(),
            label: "Search Depth".into(),
            description: "How many moves ahead to search (higher = stronger but slower)".into(),
            param_type: AiParameterDefParamType::Integer,
            default_value: "6".into(),
            min_value: Some("1".into()),
            max_value: Some("6".into()),
        }]
    }

    fn generate_move(
        &self,
        board: &BoardState,
        color: &PieceColor,
        variant: &GameVariant,
        move_history: &[Move],
        params: &HashMap<String, String>,
    ) -> Option<Move> {
        let depth: u8 = params
            .get("depth")
            .and_then(|v| v.parse().ok())
            .unwrap_or(3)
            .clamp(1, 6);

        chess_engine::best_move(board, color, variant, move_history, depth)
    }
}
