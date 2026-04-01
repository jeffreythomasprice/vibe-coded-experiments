use chess_shared::*;

use crate::board::*;
use crate::movegen::*;
use crate::variants;

/// Compute game status for the active player (the player whose turn it is).
pub fn compute_status(
    board_state: &BoardState,
    active_color: &PieceColor,
    variant: &GameVariant,
    move_history: &[Move],
) -> GameStatus {
    let board = Board::from_board_state(board_state);

    match variant {
        GameVariant::Standard => standard_status(&board, active_color, move_history),
        GameVariant::ForcedCaptureCheckmate => {
            // Same check/checkmate/stalemate rules as standard, but forced capture on moves
            standard_status(&board, active_color, move_history)
        }
        GameVariant::ForcedCaptureLoseAll => {
            lose_all_status(&board, active_color, move_history)
        }
    }
}

fn standard_status(board: &Board, active_color: &PieceColor, move_history: &[Move]) -> GameStatus {
    let enemy = opposite_color(active_color);
    let pseudo = generate_pseudo_legal_moves(board, active_color, move_history);
    let legal = filter_legal_moves(board, active_color, pseudo);

    let in_check = if let Some(king_sq) = board.find_king(active_color) {
        board.is_attacked_by(king_sq, &enemy)
    } else {
        false
    };

    if legal.is_empty() {
        if in_check {
            GameStatus::Checkmate
        } else {
            GameStatus::Stalemate
        }
    } else if in_check {
        GameStatus::Check
    } else {
        GameStatus::Active
    }
}

fn lose_all_status(board: &Board, active_color: &PieceColor, move_history: &[Move]) -> GameStatus {
    // Check if the previous player (who just moved) has lost all pieces — they win!
    let previous_color = opposite_color(active_color);
    if variants::has_no_pieces(board, &previous_color) {
        // The player who lost all pieces wins. We represent this as checkmate
        // (the previous player "checkmated" the active player by losing all pieces).
        return GameStatus::Checkmate;
    }

    // Check if active player has any moves
    let pseudo = generate_pseudo_legal_moves(board, active_color, move_history);
    let legal = variants::forced_capture_filter_lose_all(board, active_color, pseudo);

    if legal.is_empty() {
        // No legal moves = stalemate/draw in lose-all
        GameStatus::Stalemate
    } else {
        GameStatus::Active
    }
}
