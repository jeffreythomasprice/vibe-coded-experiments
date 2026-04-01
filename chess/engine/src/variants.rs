use chess_shared::*;

use crate::board::*;

/// For forced_capture_checkmate: normal legality filtering, then force captures if any exist.
pub fn forced_capture_filter(legal_moves: Vec<InternalMove>, board: &Board) -> Vec<InternalMove> {
    let captures: Vec<InternalMove> = legal_moves
        .iter()
        .filter(|m| is_capture(m, board))
        .cloned()
        .collect();

    if captures.is_empty() {
        legal_moves
    } else {
        captures
    }
}

/// For forced_capture_lose_all: king is a normal piece (no check concept),
/// so we skip the "leaves king in check" filter. Still force captures if any exist.
pub fn forced_capture_filter_lose_all(
    board: &Board,
    color: &PieceColor,
    pseudo_moves: Vec<InternalMove>,
) -> Vec<InternalMove> {
    // In lose-all, king is a normal piece — no check filtering needed.
    // But we still need to filter out moves that are blocked by our own pieces
    // (which generate_pseudo_legal_moves already handles).
    // We also need to remove self-captures which aren't possible (also already handled).
    // However we do NOT filter for leaving king in check since king is normal.
    let all_moves = filter_moves_no_check(board, color, pseudo_moves);

    let captures: Vec<InternalMove> = all_moves
        .iter()
        .filter(|m| is_capture(m, board))
        .cloned()
        .collect();

    if captures.is_empty() {
        all_moves
    } else {
        captures
    }
}

/// Filter pseudo-legal moves without checking if king is left in check.
/// Used for forced_capture_lose_all where king is a normal piece.
fn filter_moves_no_check(
    _board: &Board,
    _color: &PieceColor,
    pseudo_moves: Vec<InternalMove>,
) -> Vec<InternalMove> {
    // Pseudo-legal moves already respect piece movement and friendly blocking.
    // In lose-all variant, we don't care about check, so all pseudo-legal moves are legal.
    pseudo_moves
}

/// Check if a move is a capture (target square has an enemy piece, or en passant).
fn is_capture(m: &InternalMove, board: &Board) -> bool {
    // Normal capture: target square has a piece
    if board.get(m.to).is_some() {
        return true;
    }

    // En passant: pawn moves diagonally to empty square
    if let Some(piece) = board.get(m.from) {
        if piece.piece_type == PieceType::Pawn && m.from.file != m.to.file {
            return true;
        }
    }

    false
}

/// Check if a player has lost all their pieces (win condition for forced_capture_lose_all).
pub fn has_no_pieces(board: &Board, color: &PieceColor) -> bool {
    for idx in 0..64 {
        if let Some(piece) = &board.squares[idx] {
            if piece.color == *color {
                return false;
            }
        }
    }
    true
}
