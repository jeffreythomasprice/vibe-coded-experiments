use chess_shared::*;

use crate::board::*;

/// Generate all pseudo-legal moves for the given color.
/// These respect piece movement rules but may leave the king in check.
pub fn generate_pseudo_legal_moves(
    board: &Board,
    color: &PieceColor,
    move_history: &[Move],
) -> Vec<InternalMove> {
    let mut moves = Vec::new();
    let castling = CastlingRights::from_history(move_history);
    let ep_target = en_passant_target(move_history);

    for idx in 0..64 {
        if let Some(piece) = &board.squares[idx] {
            if piece.color != *color {
                continue;
            }
            let sq = Square::from_index(idx);
            match piece.piece_type {
                PieceType::Pawn => gen_pawn_moves(board, sq, color, ep_target, &mut moves),
                PieceType::Knight => gen_knight_moves(board, sq, color, &mut moves),
                PieceType::Bishop => gen_sliding_moves(board, sq, color, &DIAG_DIRS, &mut moves),
                PieceType::Rook => gen_sliding_moves(board, sq, color, &STRAIGHT_DIRS, &mut moves),
                PieceType::Queen => {
                    gen_sliding_moves(board, sq, color, &DIAG_DIRS, &mut moves);
                    gen_sliding_moves(board, sq, color, &STRAIGHT_DIRS, &mut moves);
                }
                PieceType::King => gen_king_moves(board, sq, color, &castling, &mut moves),
            }
        }
    }

    moves
}

const DIAG_DIRS: [(i8, i8); 4] = [(-1, -1), (-1, 1), (1, -1), (1, 1)];
const STRAIGHT_DIRS: [(i8, i8); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];

fn gen_pawn_moves(
    board: &Board,
    sq: Square,
    color: &PieceColor,
    ep_target: Option<Square>,
    moves: &mut Vec<InternalMove>,
) {
    let (dir, start_rank, promo_rank): (i8, u8, u8) = match color {
        PieceColor::White => (1, 1, 7),
        PieceColor::Black => (-1, 6, 0),
    };

    // Single push
    if let Some(to) = sq.offset(0, dir) {
        if board.get(to).is_none() {
            if to.rank == promo_rank {
                add_promotions(sq, to, moves);
            } else {
                moves.push(InternalMove {
                    from: sq,
                    to,
                    promotion: None,
                });

                // Double push from starting rank
                if sq.rank == start_rank {
                    if let Some(to2) = sq.offset(0, dir * 2) {
                        if board.get(to2).is_none() {
                            moves.push(InternalMove {
                                from: sq,
                                to: to2,
                                promotion: None,
                            });
                        }
                    }
                }
            }
        }
    }

    // Captures (including en passant)
    for df in [-1i8, 1] {
        if let Some(to) = sq.offset(df, dir) {
            let is_capture = board
                .get(to)
                .is_some_and(|p| p.color != *color);
            let is_ep = ep_target == Some(to);

            if is_capture || is_ep {
                if to.rank == promo_rank {
                    add_promotions(sq, to, moves);
                } else {
                    moves.push(InternalMove {
                        from: sq,
                        to,
                        promotion: None,
                    });
                }
            }
        }
    }
}

fn add_promotions(from: Square, to: Square, moves: &mut Vec<InternalMove>) {
    for piece_type in [
        PieceType::Queen,
        PieceType::Rook,
        PieceType::Bishop,
        PieceType::Knight,
    ] {
        moves.push(InternalMove {
            from,
            to,
            promotion: Some(piece_type),
        });
    }
}

fn gen_knight_moves(board: &Board, sq: Square, color: &PieceColor, moves: &mut Vec<InternalMove>) {
    let offsets = [
        (-2, -1),
        (-2, 1),
        (-1, -2),
        (-1, 2),
        (1, -2),
        (1, 2),
        (2, -1),
        (2, 1),
    ];
    for (df, dr) in offsets {
        if let Some(to) = sq.offset(df, dr) {
            match board.get(to) {
                Some(p) if p.color == *color => continue,
                _ => moves.push(InternalMove {
                    from: sq,
                    to,
                    promotion: None,
                }),
            }
        }
    }
}

fn gen_sliding_moves(
    board: &Board,
    sq: Square,
    color: &PieceColor,
    dirs: &[(i8, i8)],
    moves: &mut Vec<InternalMove>,
) {
    for &(df, dr) in dirs {
        let mut cur = sq;
        loop {
            cur = match cur.offset(df, dr) {
                Some(s) => s,
                None => break,
            };
            match board.get(cur) {
                None => {
                    moves.push(InternalMove {
                        from: sq,
                        to: cur,
                        promotion: None,
                    });
                }
                Some(p) => {
                    if p.color != *color {
                        moves.push(InternalMove {
                            from: sq,
                            to: cur,
                            promotion: None,
                        });
                    }
                    break;
                }
            }
        }
    }
}

fn gen_king_moves(
    board: &Board,
    sq: Square,
    color: &PieceColor,
    castling: &CastlingRights,
    moves: &mut Vec<InternalMove>,
) {
    // Normal king moves
    for df in -1..=1i8 {
        for dr in -1..=1i8 {
            if df == 0 && dr == 0 {
                continue;
            }
            if let Some(to) = sq.offset(df, dr) {
                match board.get(to) {
                    Some(p) if p.color == *color => continue,
                    _ => moves.push(InternalMove {
                        from: sq,
                        to,
                        promotion: None,
                    }),
                }
            }
        }
    }

    // Castling
    let enemy = opposite_color(color);
    match color {
        PieceColor::White => {
            if castling.white_kingside && sq == Square::new(4, 0)
                && can_castle_kingside(board, 0, &enemy)
            {
                moves.push(InternalMove {
                    from: sq,
                    to: Square::new(6, 0),
                    promotion: None,
                });
            }
            if castling.white_queenside && sq == Square::new(4, 0)
                && can_castle_queenside(board, 0, &enemy)
            {
                moves.push(InternalMove {
                    from: sq,
                    to: Square::new(2, 0),
                    promotion: None,
                });
            }
        }
        PieceColor::Black => {
            if castling.black_kingside && sq == Square::new(4, 7)
                && can_castle_kingside(board, 7, &enemy)
            {
                moves.push(InternalMove {
                    from: sq,
                    to: Square::new(6, 7),
                    promotion: None,
                });
            }
            if castling.black_queenside && sq == Square::new(4, 7)
                && can_castle_queenside(board, 7, &enemy)
            {
                moves.push(InternalMove {
                    from: sq,
                    to: Square::new(2, 7),
                    promotion: None,
                });
            }
        }
    }
}

fn can_castle_kingside(board: &Board, rank: u8, enemy: &PieceColor) -> bool {
    // f and g files must be empty
    let f = Square::new(5, rank);
    let g = Square::new(6, rank);
    if board.get(f).is_some() || board.get(g).is_some() {
        return false;
    }
    // King's current square, f, and g must not be attacked
    let king = Square::new(4, rank);
    !board.is_attacked_by(king, enemy)
        && !board.is_attacked_by(f, enemy)
        && !board.is_attacked_by(g, enemy)
}

fn can_castle_queenside(board: &Board, rank: u8, enemy: &PieceColor) -> bool {
    // b, c, and d files must be empty
    let b = Square::new(1, rank);
    let c = Square::new(2, rank);
    let d = Square::new(3, rank);
    if board.get(b).is_some() || board.get(c).is_some() || board.get(d).is_some() {
        return false;
    }
    // King's current square, c, and d must not be attacked
    let king = Square::new(4, rank);
    !board.is_attacked_by(king, enemy)
        && !board.is_attacked_by(c, enemy)
        && !board.is_attacked_by(d, enemy)
}

/// Filter pseudo-legal moves to only legal moves (don't leave own king in check).
pub fn filter_legal_moves(
    board: &Board,
    color: &PieceColor,
    pseudo_moves: Vec<InternalMove>,
) -> Vec<InternalMove> {
    let enemy = opposite_color(color);
    pseudo_moves
        .into_iter()
        .filter(|m| {
            let mut test_board = board.clone();
            apply_internal_move(&mut test_board, m);
            // After making the move, our king must not be in check
            if let Some(king_sq) = test_board.find_king(color) {
                !test_board.is_attacked_by(king_sq, &enemy)
            } else {
                // King doesn't exist (shouldn't happen in standard chess)
                true
            }
        })
        .collect()
}

/// Apply a move to the internal board representation.
pub fn apply_internal_move(board: &mut Board, m: &InternalMove) {
    let piece = board.get(m.from).cloned();
    let Some(mut piece) = piece else { return };

    // Handle en passant capture
    if piece.piece_type == PieceType::Pawn && m.from.file != m.to.file && board.get(m.to).is_none()
    {
        // Diagonal pawn move to empty square = en passant
        let captured_sq = Square::new(m.to.file, m.from.rank);
        board.set(captured_sq, None);
    }

    // Handle castling (move the rook)
    if piece.piece_type == PieceType::King {
        let file_diff = m.to.file as i8 - m.from.file as i8;
        if file_diff.abs() == 2 {
            // Castling
            let rank = m.from.rank;
            if file_diff > 0 {
                // Kingside
                let rook = board.get(Square::new(7, rank)).cloned();
                board.set(Square::new(7, rank), None);
                board.set(Square::new(5, rank), rook);
            } else {
                // Queenside
                let rook = board.get(Square::new(0, rank)).cloned();
                board.set(Square::new(0, rank), None);
                board.set(Square::new(3, rank), rook);
            }
        }
    }

    // Handle promotion
    if let Some(promo_type) = &m.promotion {
        piece.piece_type = *promo_type;
    }

    board.set(m.from, None);
    board.set(m.to, Some(piece));
}
