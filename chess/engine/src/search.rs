use chess_shared::*;

use crate::board::*;
use crate::eval;
use crate::movegen;
use crate::variants;

pub struct SearchParams {
    pub max_depth: u8,
}

pub struct SearchResult {
    pub best_move: Option<InternalMove>,
    pub score: i32,
    pub nodes_searched: u64,
}

const INF: i32 = 1_000_000;
const CHECKMATE_SCORE: i32 = 999_000;

/// Find the best move using alpha-beta minimax search.
pub fn search(
    board: &Board,
    color: &PieceColor,
    variant: &GameVariant,
    move_history: &[Move],
    params: &SearchParams,
) -> SearchResult {
    let mut nodes = 0u64;
    let legal = generate_legal_moves(board, color, variant, move_history);

    if legal.is_empty() {
        return SearchResult {
            best_move: None,
            score: 0,
            nodes_searched: 0,
        };
    }

    let ordered = order_moves(&legal, board);

    let mut best_move = None;
    let mut best_score = -INF;
    let mut alpha = -INF;
    let beta = INF;

    for m in &ordered {
        let mut new_board = board.clone();
        movegen::apply_internal_move(&mut new_board, m);

        let mut new_history = move_history.to_vec();
        new_history.push(m.to_schema_move());

        let opponent = opposite_color(color);

        // Check terminal state before recursing
        let score = if is_terminal_after_move(&new_board, &opponent, variant, color, &new_history) {
            terminal_score(&new_board, variant, color, params.max_depth)
        } else {
            -negamax(
                &new_board,
                &opponent,
                variant,
                &new_history,
                params.max_depth - 1,
                -beta,
                -alpha,
                &mut nodes,
            )
        };

        nodes += 1;

        if score > best_score {
            best_score = score;
            best_move = Some(m.clone());
        }
        if score > alpha {
            alpha = score;
        }
    }

    SearchResult {
        best_move,
        score: best_score,
        nodes_searched: nodes,
    }
}

fn negamax(
    board: &Board,
    color: &PieceColor,
    variant: &GameVariant,
    move_history: &[Move],
    depth: u8,
    mut alpha: i32,
    beta: i32,
    nodes: &mut u64,
) -> i32 {
    if depth == 0 {
        *nodes += 1;
        return eval::evaluate(board, variant, color);
    }

    let legal = generate_legal_moves(board, color, variant, move_history);

    if legal.is_empty() {
        *nodes += 1;
        return terminal_score_no_moves(board, variant, color);
    }

    let ordered = order_moves(&legal, board);
    let mut best_score = -INF;

    for m in &ordered {
        let mut new_board = board.clone();
        movegen::apply_internal_move(&mut new_board, m);

        let mut new_history = move_history.to_vec();
        new_history.push(m.to_schema_move());

        let opponent = opposite_color(color);

        let score = if is_terminal_after_move(&new_board, &opponent, variant, color, &new_history) {
            terminal_score(&new_board, variant, color, depth)
        } else {
            -negamax(
                &new_board,
                &opponent,
                variant,
                &new_history,
                depth - 1,
                -beta,
                -alpha,
                nodes,
            )
        };

        *nodes += 1;

        if score > best_score {
            best_score = score;
        }
        if score > alpha {
            alpha = score;
        }
        if alpha >= beta {
            break; // Beta cutoff
        }
    }

    best_score
}

/// Generate legal moves for the given position, respecting variant rules.
fn generate_legal_moves(
    board: &Board,
    color: &PieceColor,
    variant: &GameVariant,
    move_history: &[Move],
) -> Vec<InternalMove> {
    let pseudo = movegen::generate_pseudo_legal_moves(board, color, move_history);
    match variant {
        GameVariant::Standard => movegen::filter_legal_moves(board, color, pseudo),
        GameVariant::ForcedCaptureLoseAll => {
            variants::forced_capture_filter_lose_all(board, color, pseudo)
        }
        GameVariant::ForcedCaptureCheckmate => {
            let legal = movegen::filter_legal_moves(board, color, pseudo);
            variants::forced_capture_filter(legal, board)
        }
    }
}

/// Check if the position is terminal after a move was made.
/// `mover` is the color that just moved, `next` is the color to move.
fn is_terminal_after_move(
    board: &Board,
    next_color: &PieceColor,
    variant: &GameVariant,
    mover_color: &PieceColor,
    move_history: &[Move],
) -> bool {
    match variant {
        GameVariant::ForcedCaptureLoseAll => {
            // Check if mover lost all pieces (they win)
            if variants::has_no_pieces(board, mover_color) {
                return true;
            }
            // Check if next player has no moves
            let legal = generate_legal_moves(board, next_color, variant, move_history);
            legal.is_empty()
        }
        _ => {
            // Check if next player has no legal moves (checkmate or stalemate)
            let legal = generate_legal_moves(board, next_color, variant, move_history);
            legal.is_empty()
        }
    }
}

/// Score a terminal position from the perspective of `mover_color` (who just moved).
fn terminal_score(board: &Board, variant: &GameVariant, mover_color: &PieceColor, depth: u8) -> i32 {
    let depth_bonus = depth as i32; // Prefer faster wins

    match variant {
        GameVariant::ForcedCaptureLoseAll => {
            if variants::has_no_pieces(board, mover_color) {
                // Mover lost all pieces = mover wins
                CHECKMATE_SCORE + depth_bonus
            } else {
                // Next player has no moves = stalemate = draw
                0
            }
        }
        _ => {
            let next_color = opposite_color(mover_color);
            let enemy = opposite_color(&next_color);
            let in_check = board
                .find_king(&next_color)
                .map_or(false, |ksq| board.is_attacked_by(ksq, &enemy));

            if in_check {
                // Checkmate — mover wins
                CHECKMATE_SCORE + depth_bonus
            } else {
                // Stalemate — draw
                0
            }
        }
    }
}

/// Score when the current side to move has no legal moves.
fn terminal_score_no_moves(board: &Board, variant: &GameVariant, color: &PieceColor) -> i32 {
    match variant {
        GameVariant::ForcedCaptureLoseAll => {
            // No moves = stalemate in lose-all
            0
        }
        _ => {
            let enemy = opposite_color(color);
            let in_check = board
                .find_king(color)
                .map_or(false, |ksq| board.is_attacked_by(ksq, &enemy));

            if in_check {
                // We're checkmated
                -(CHECKMATE_SCORE)
            } else {
                // Stalemate
                0
            }
        }
    }
}

/// Order moves for better alpha-beta pruning using MVV-LVA heuristic.
/// Captures are sorted first (most valuable victim, least valuable attacker).
fn order_moves(moves: &[InternalMove], board: &Board) -> Vec<InternalMove> {
    let mut scored: Vec<(i32, &InternalMove)> = moves
        .iter()
        .map(|m| {
            let score = move_order_score(m, board);
            (score, m)
        })
        .collect();

    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.into_iter().map(|(_, m)| m.clone()).collect()
}

fn move_order_score(m: &InternalMove, board: &Board) -> i32 {
    // Captures: MVV-LVA (Most Valuable Victim - Least Valuable Attacker)
    if let Some(victim) = board.get(m.to) {
        let victim_val = eval::piece_value_for_ordering(&victim.piece_type);
        let attacker_val = board
            .get(m.from)
            .map(|p| eval::piece_value_for_ordering(&p.piece_type))
            .unwrap_or(0);
        return 10_000 + victim_val - attacker_val / 10;
    }

    // En passant captures (pawn moves diagonally to empty square)
    if let Some(piece) = board.get(m.from) {
        if piece.piece_type == PieceType::Pawn && m.from.file != m.to.file {
            return 10_000 + PAWN_CAPTURE_BONUS;
        }
    }

    // Promotions
    if m.promotion.is_some() {
        return 9_000;
    }

    0
}

const PAWN_CAPTURE_BONUS: i32 = 100;
