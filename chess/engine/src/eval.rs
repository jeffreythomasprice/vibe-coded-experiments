use chess_shared::*;

use crate::board::*;

// Material values in centipawns
const PAWN_VALUE: i32 = 100;
const KNIGHT_VALUE: i32 = 320;
const BISHOP_VALUE: i32 = 330;
const ROOK_VALUE: i32 = 500;
const QUEEN_VALUE: i32 = 900;
const KING_VALUE: i32 = 20000;

fn piece_value(piece_type: &PieceType) -> i32 {
    match piece_type {
        PieceType::Pawn => PAWN_VALUE,
        PieceType::Knight => KNIGHT_VALUE,
        PieceType::Bishop => BISHOP_VALUE,
        PieceType::Rook => ROOK_VALUE,
        PieceType::Queen => QUEEN_VALUE,
        PieceType::King => KING_VALUE,
    }
}

/// Piece value used for MVV-LVA move ordering (excludes king).
pub fn piece_value_for_ordering(piece_type: &PieceType) -> i32 {
    match piece_type {
        PieceType::Pawn => PAWN_VALUE,
        PieceType::Knight => KNIGHT_VALUE,
        PieceType::Bishop => BISHOP_VALUE,
        PieceType::Rook => ROOK_VALUE,
        PieceType::Queen => QUEEN_VALUE,
        PieceType::King => 0, // King captures don't get MVV-LVA bonus
    }
}

// Piece-square tables indexed by square (rank*8 + file) from White's perspective.
// Values from https://www.chessprogramming.org/Simplified_Evaluation_Function

#[rustfmt::skip]
const PAWN_PST: [i32; 64] = [
     0,  0,  0,  0,  0,  0,  0,  0,
    50, 50, 50, 50, 50, 50, 50, 50,
    10, 10, 20, 30, 30, 20, 10, 10,
     5,  5, 10, 25, 25, 10,  5,  5,
     0,  0,  0, 20, 20,  0,  0,  0,
     5, -5,-10,  0,  0,-10, -5,  5,
     5, 10, 10,-20,-20, 10, 10,  5,
     0,  0,  0,  0,  0,  0,  0,  0,
];

#[rustfmt::skip]
const KNIGHT_PST: [i32; 64] = [
    -50,-40,-30,-30,-30,-30,-40,-50,
    -40,-20,  0,  0,  0,  0,-20,-40,
    -30,  0, 10, 15, 15, 10,  0,-30,
    -30,  5, 15, 20, 20, 15,  5,-30,
    -30,  0, 15, 20, 20, 15,  0,-30,
    -30,  5, 10, 15, 15, 10,  5,-30,
    -40,-20,  0,  5,  5,  0,-20,-40,
    -50,-40,-30,-30,-30,-30,-40,-50,
];

#[rustfmt::skip]
const BISHOP_PST: [i32; 64] = [
    -20,-10,-10,-10,-10,-10,-10,-20,
    -10,  0,  0,  0,  0,  0,  0,-10,
    -10,  0,  5, 10, 10,  5,  0,-10,
    -10,  5,  5, 10, 10,  5,  5,-10,
    -10,  0, 10, 10, 10, 10,  0,-10,
    -10, 10, 10, 10, 10, 10, 10,-10,
    -10,  5,  0,  0,  0,  0,  5,-10,
    -20,-10,-10,-10,-10,-10,-10,-20,
];

#[rustfmt::skip]
const ROOK_PST: [i32; 64] = [
     0,  0,  0,  0,  0,  0,  0,  0,
     5, 10, 10, 10, 10, 10, 10,  5,
    -5,  0,  0,  0,  0,  0,  0, -5,
    -5,  0,  0,  0,  0,  0,  0, -5,
    -5,  0,  0,  0,  0,  0,  0, -5,
    -5,  0,  0,  0,  0,  0,  0, -5,
    -5,  0,  0,  0,  0,  0,  0, -5,
     0,  0,  0,  5,  5,  0,  0,  0,
];

#[rustfmt::skip]
const QUEEN_PST: [i32; 64] = [
    -20,-10,-10, -5, -5,-10,-10,-20,
    -10,  0,  0,  0,  0,  0,  0,-10,
    -10,  0,  5,  5,  5,  5,  0,-10,
     -5,  0,  5,  5,  5,  5,  0, -5,
      0,  0,  5,  5,  5,  5,  0, -5,
    -10,  5,  5,  5,  5,  5,  0,-10,
    -10,  0,  5,  0,  0,  0,  0,-10,
    -20,-10,-10, -5, -5,-10,-10,-20,
];

#[rustfmt::skip]
const KING_MIDDLEGAME_PST: [i32; 64] = [
    -30,-40,-40,-50,-50,-40,-40,-30,
    -30,-40,-40,-50,-50,-40,-40,-30,
    -30,-40,-40,-50,-50,-40,-40,-30,
    -30,-40,-40,-50,-50,-40,-40,-30,
    -20,-30,-30,-40,-40,-30,-30,-20,
    -10,-20,-20,-20,-20,-20,-20,-10,
     20, 20,  0,  0,  0,  0, 20, 20,
     20, 30, 10,  0,  0, 10, 30, 20,
];

fn pst_value(piece_type: &PieceType, sq: Square, color: &PieceColor) -> i32 {
    // Tables are stored from White's perspective with rank 7 at top (index 0).
    // For white: index = (7 - rank) * 8 + file
    // For black: mirror = rank * 8 + file
    let idx = match color {
        PieceColor::White => (7 - sq.rank as usize) * 8 + sq.file as usize,
        PieceColor::Black => sq.rank as usize * 8 + sq.file as usize,
    };

    match piece_type {
        PieceType::Pawn => PAWN_PST[idx],
        PieceType::Knight => KNIGHT_PST[idx],
        PieceType::Bishop => BISHOP_PST[idx],
        PieceType::Rook => ROOK_PST[idx],
        PieceType::Queen => QUEEN_PST[idx],
        PieceType::King => KING_MIDDLEGAME_PST[idx],
    }
}

/// Evaluate a board position from the perspective of `active_color`.
/// Positive = good for active_color, negative = bad.
pub fn evaluate(board: &Board, variant: &GameVariant, active_color: &PieceColor) -> i32 {
    let score = match variant {
        GameVariant::Standard | GameVariant::ForcedCaptureCheckmate => {
            evaluate_standard(board)
        }
        GameVariant::ForcedCaptureLoseAll => {
            evaluate_lose_all(board)
        }
    };

    // Convert to active_color perspective
    match active_color {
        PieceColor::White => score,
        PieceColor::Black => -score,
    }
}

/// Standard evaluation: material + positional. Positive = good for white.
fn evaluate_standard(board: &Board) -> i32 {
    let mut score = 0;

    for idx in 0..64 {
        if let Some(piece) = &board.squares[idx] {
            let sq = Square::from_index(idx);
            let material = piece_value(&piece.piece_type);
            let positional = pst_value(&piece.piece_type, sq, &piece.color);

            let piece_score = material + positional;
            match piece.color {
                PieceColor::White => score += piece_score,
                PieceColor::Black => score -= piece_score,
            }
        }
    }

    score
}

/// Lose-all evaluation: you WANT to lose pieces. Fewer own pieces = better.
/// Positive = good for white (white has fewer pieces / opponent has more).
fn evaluate_lose_all(board: &Board) -> i32 {
    let mut white_material = 0;
    let mut black_material = 0;

    for idx in 0..64 {
        if let Some(piece) = &board.squares[idx] {
            let val = piece_value(&piece.piece_type);
            match piece.color {
                PieceColor::White => white_material += val,
                PieceColor::Black => black_material += val,
            }
        }
    }

    // Invert: having less material is good, opponent having more is good
    black_material - white_material
}
