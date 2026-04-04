pub mod board;
mod eval;
pub mod movegen;
mod search;
pub mod status;
pub mod variants;

use chess_shared::*;

/// Find the best move using alpha-beta minimax search.
/// Returns None if no legal moves exist.
pub fn best_move(
    board_state: &BoardState,
    color: &PieceColor,
    variant: &GameVariant,
    move_history: &[Move],
    depth: u8,
) -> Option<Move> {
    let b = board::Board::from_board_state(board_state);
    let params = search::SearchParams { max_depth: depth };
    let position_keys = board::build_position_keys(move_history);
    let result = search::search(&b, color, variant, move_history, &params, &position_keys);
    result.best_move.map(|m| m.to_schema_move())
}

/// Build position keys from move history for repetition detection.
pub fn build_position_keys(move_history: &[Move]) -> Vec<board::PositionKey> {
    board::build_position_keys(move_history)
}

/// Check if applying a move would lead to a position that already appeared in the game.
pub fn would_repeat(
    board_state: &BoardState,
    chess_move: &Move,
    active_color: &PieceColor,
    move_history: &[Move],
    position_keys: &[board::PositionKey],
) -> bool {
    let mut b = board::Board::from_board_state(board_state);
    let im = board::InternalMove::from_schema_move(chess_move);
    movegen::apply_internal_move(&mut b, &im);
    let next_color = board::opposite_color(active_color);
    let mut extended_history = move_history.to_vec();
    extended_history.push(chess_move.clone());
    let key = board::PositionKey::from_board(&b, &next_color, &extended_history);
    position_keys.contains(&key)
}

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
