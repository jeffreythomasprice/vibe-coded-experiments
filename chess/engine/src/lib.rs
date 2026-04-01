pub mod board;
pub mod movegen;
pub mod status;
pub mod variants;

use chess_shared::*;

/// Returns all legal moves for the active color in the given position.
pub fn legal_moves(
    board: &BoardState,
    active_color: &PieceColor,
    variant: &GameVariant,
    move_history: &[Move],
) -> Vec<Move> {
    let b = board::Board::from_board_state(board);
    let pseudo = movegen::generate_pseudo_legal_moves(&b, active_color, move_history);

    let filtered = match variant {
        GameVariant::Standard => {
            movegen::filter_legal_moves(&b, active_color, pseudo)
        }
        GameVariant::ForcedCaptureLoseAll => {
            variants::forced_capture_filter_lose_all(&b, active_color, pseudo)
        }
        GameVariant::ForcedCaptureCheckmate => {
            let legal = movegen::filter_legal_moves(&b, active_color, pseudo);
            variants::forced_capture_filter(legal, &b)
        }
    };

    filtered
        .into_iter()
        .map(|m| m.to_schema_move())
        .collect()
}

/// Applies a move to the board state, mutating it in place.
pub fn apply_move(board: &mut BoardState, m: &Move) {
    let mut b = board::Board::from_board_state(board);
    let im = board::InternalMove::from_schema_move(m);
    movegen::apply_internal_move(&mut b, &im);
    *board = b.to_board_state();
}

/// Determines the game status after a move has been applied.
pub fn game_status(
    board: &BoardState,
    active_color: &PieceColor,
    variant: &GameVariant,
    move_history: &[Move],
) -> GameStatus {
    status::compute_status(board, active_color, variant, move_history)
}
