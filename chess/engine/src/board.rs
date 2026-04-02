use chess_shared::*;
use std::collections::HashMap;

/// Internal square representation (file 0-7, rank 0-7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Square {
    pub file: u8, // 0=a, 7=h
    pub rank: u8, // 0=1, 7=8
}

impl Square {
    pub fn new(file: u8, rank: u8) -> Self {
        Self { file, rank }
    }

    pub fn from_algebraic(s: &str) -> Option<Self> {
        let bytes = s.as_bytes();
        if bytes.len() != 2 {
            return None;
        }
        let file = bytes[0].checked_sub(b'a')?;
        let rank = bytes[1].checked_sub(b'1')?;
        if file > 7 || rank > 7 {
            return None;
        }
        Some(Self { file, rank })
    }

    pub fn to_algebraic(self) -> String {
        let f = (b'a' + self.file) as char;
        let r = (b'1' + self.rank) as char;
        format!("{f}{r}")
    }

    pub fn index(self) -> usize {
        (self.rank as usize) * 8 + (self.file as usize)
    }

    pub fn from_index(idx: usize) -> Self {
        Self {
            file: (idx % 8) as u8,
            rank: (idx / 8) as u8,
        }
    }

    pub fn offset(self, df: i8, dr: i8) -> Option<Self> {
        let f = self.file as i8 + df;
        let r = self.rank as i8 + dr;
        if !(0..=7).contains(&f) || !(0..=7).contains(&r) {
            None
        } else {
            Some(Self {
                file: f as u8,
                rank: r as u8,
            })
        }
    }
}

/// Internal move representation using Square instead of the validated string types.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InternalMove {
    pub from: Square,
    pub to: Square,
    pub promotion: Option<PieceType>,
}

impl InternalMove {
    pub fn from_schema_move(m: &Move) -> Self {
        Self {
            from: Square::from_algebraic(&m.from).unwrap(),
            to: Square::from_algebraic(&m.to).unwrap(),
            promotion: m.promotion.as_ref().map(|p| match p {
                MovePromotion::Queen => PieceType::Queen,
                MovePromotion::Rook => PieceType::Rook,
                MovePromotion::Bishop => PieceType::Bishop,
                MovePromotion::Knight => PieceType::Knight,
            }),
        }
    }

    pub fn to_schema_move(&self) -> Move {
        Move {
            from: self.from.to_algebraic().parse().unwrap(),
            to: self.to.to_algebraic().parse().unwrap(),
            promotion: self.promotion.as_ref().map(|p| match p {
                PieceType::Queen => MovePromotion::Queen,
                PieceType::Rook => MovePromotion::Rook,
                PieceType::Bishop => MovePromotion::Bishop,
                PieceType::Knight => MovePromotion::Knight,
                _ => MovePromotion::Queen,
            }),
        }
    }
}

/// Efficient 8x8 board representation.
#[derive(Clone, Debug)]
pub struct Board {
    pub squares: [Option<Piece>; 64],
}

impl Default for Board {
    fn default() -> Self {
        Self {
            squares: [const { None }; 64],
        }
    }
}

impl Board {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, sq: Square) -> Option<&Piece> {
        self.squares[sq.index()].as_ref()
    }

    pub fn set(&mut self, sq: Square, piece: Option<Piece>) {
        self.squares[sq.index()] = piece;
    }

    pub fn from_board_state(state: &BoardState) -> Self {
        let mut board = Self::new();
        for (sq_str, piece) in &state.pieces {
            if let Some(sq) = Square::from_algebraic(sq_str) {
                board.set(sq, Some(piece.clone()));
            }
        }
        board
    }

    pub fn to_board_state(&self) -> BoardState {
        let mut pieces = HashMap::new();
        for idx in 0..64 {
            if let Some(piece) = &self.squares[idx] {
                let sq = Square::from_index(idx);
                pieces.insert(sq.to_algebraic(), piece.clone());
            }
        }
        BoardState { pieces }
    }

    /// Find the king square for the given color.
    pub fn find_king(&self, color: &PieceColor) -> Option<Square> {
        for idx in 0..64 {
            if let Some(piece) = &self.squares[idx] {
                if piece.piece_type == PieceType::King && piece.color == *color {
                    return Some(Square::from_index(idx));
                }
            }
        }
        None
    }

    /// Check if a square is attacked by the given color.
    pub fn is_attacked_by(&self, sq: Square, attacker_color: &PieceColor) -> bool {
        // Check pawn attacks
        let pawn_dir: i8 = if *attacker_color == PieceColor::White {
            -1
        } else {
            1
        };
        for df in [-1i8, 1] {
            if let Some(from) = sq.offset(df, pawn_dir) {
                if let Some(piece) = self.get(from) {
                    if piece.color == *attacker_color && piece.piece_type == PieceType::Pawn {
                        return true;
                    }
                }
            }
        }

        // Check knight attacks
        let knight_offsets = [
            (-2, -1),
            (-2, 1),
            (-1, -2),
            (-1, 2),
            (1, -2),
            (1, 2),
            (2, -1),
            (2, 1),
        ];
        for (df, dr) in knight_offsets {
            if let Some(from) = sq.offset(df, dr) {
                if let Some(piece) = self.get(from) {
                    if piece.color == *attacker_color && piece.piece_type == PieceType::Knight {
                        return true;
                    }
                }
            }
        }

        // Check king attacks
        for df in -1..=1i8 {
            for dr in -1..=1i8 {
                if df == 0 && dr == 0 {
                    continue;
                }
                if let Some(from) = sq.offset(df, dr) {
                    if let Some(piece) = self.get(from) {
                        if piece.color == *attacker_color && piece.piece_type == PieceType::King {
                            return true;
                        }
                    }
                }
            }
        }

        // Check sliding pieces (bishop/queen diagonals, rook/queen straights)
        let diag_dirs = [(-1, -1), (-1, 1), (1, -1), (1, 1)];
        for (df, dr) in diag_dirs {
            let mut cur = sq;
            loop {
                cur = match cur.offset(df, dr) {
                    Some(s) => s,
                    None => break,
                };
                match self.get(cur) {
                    Some(piece) => {
                        if piece.color == *attacker_color
                            && (piece.piece_type == PieceType::Bishop
                                || piece.piece_type == PieceType::Queen)
                        {
                            return true;
                        }
                        break;
                    }
                    None => continue,
                }
            }
        }

        let straight_dirs = [(-1, 0), (1, 0), (0, -1), (0, 1)];
        for (df, dr) in straight_dirs {
            let mut cur = sq;
            loop {
                cur = match cur.offset(df, dr) {
                    Some(s) => s,
                    None => break,
                };
                match self.get(cur) {
                    Some(piece) => {
                        if piece.color == *attacker_color
                            && (piece.piece_type == PieceType::Rook
                                || piece.piece_type == PieceType::Queen)
                        {
                            return true;
                        }
                        break;
                    }
                    None => continue,
                }
            }
        }

        false
    }
}

/// Derive castling rights from move history.
pub struct CastlingRights {
    pub white_kingside: bool,
    pub white_queenside: bool,
    pub black_kingside: bool,
    pub black_queenside: bool,
}

impl CastlingRights {
    pub fn from_history(move_history: &[Move]) -> Self {
        let mut rights = CastlingRights {
            white_kingside: true,
            white_queenside: true,
            black_kingside: true,
            black_queenside: true,
        };

        for m in move_history {
            let from = &*m.from;
            let to = &*m.to;

            // King moved
            if from == "e1" {
                rights.white_kingside = false;
                rights.white_queenside = false;
            }
            if from == "e8" {
                rights.black_kingside = false;
                rights.black_queenside = false;
            }

            // Rook moved or captured
            if from == "h1" || to == "h1" {
                rights.white_kingside = false;
            }
            if from == "a1" || to == "a1" {
                rights.white_queenside = false;
            }
            if from == "h8" || to == "h8" {
                rights.black_kingside = false;
            }
            if from == "a8" || to == "a8" {
                rights.black_queenside = false;
            }
        }

        rights
    }
}

/// Get en passant target square from the last move, if applicable.
pub fn en_passant_target(move_history: &[Move]) -> Option<Square> {
    let last = move_history.last()?;
    let from = Square::from_algebraic(&last.from)?;
    let to = Square::from_algebraic(&last.to)?;

    // Check if it was a two-square pawn advance
    if from.file == to.file && (from.rank as i8 - to.rank as i8).unsigned_abs() == 2 {
        // The en passant target is the square the pawn passed through
        let ep_rank = (from.rank + to.rank) / 2;
        Some(Square::new(from.file, ep_rank))
    } else {
        None
    }
}

/// Get the opposite color.
pub fn opposite_color(color: &PieceColor) -> PieceColor {
    match color {
        PieceColor::White => PieceColor::Black,
        PieceColor::Black => PieceColor::White,
    }
}
