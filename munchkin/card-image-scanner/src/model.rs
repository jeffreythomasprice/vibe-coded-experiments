//! Serde data model for the TOML database output.

use serde::Serialize;

/// The top-level database written to TOML. `card` becomes a `[[card]]`
/// array-of-tables.
#[derive(Debug, Serialize)]
pub struct Database {
    /// RFC3339-ish timestamp of when the scan was generated.
    pub generated: String,
    /// Number of source sheets processed.
    pub sheets: usize,
    /// Total number of cards extracted.
    pub cards: usize,
    #[serde(rename = "card")]
    pub card_list: Vec<Card>,
}

/// A single extracted card.
#[derive(Debug, Serialize)]
pub struct Card {
    /// Stable id, e.g. `door1_r0c1`.
    pub id: String,
    /// Source sheet filename (not full path).
    pub source_image: String,
    /// `door`, `loot`, or `unknown`.
    pub card_type: String,
    pub index_in_sheet: u32,
    pub row: u32,
    pub col: u32,
    /// Location of this card within the source sheet (full-res pixels).
    pub bbox: BBox,

    // ---- structured text (best-effort) ----
    /// Canonical title (corrected against the card list) if matched, else the
    /// raw OCR title, else None.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The original OCR'd title before correction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_raw: Option<String>,
    /// Fuzzy match score [0,1] if the title was corrected against the list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_match_score: Option<f64>,
    /// Smaller lines above/around the title (e.g. "+3 Bonus", "Usable by ...").
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub top_extras: Vec<String>,
    /// Body text below the illustration, with wrapped lines stitched.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// Bottom-left corner text (e.g. "Footgear", "2 Levels").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bottom_left: Option<String>,
    /// Bottom-right corner text (e.g. "400 Gold Pieces", "4 Treasures").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bottom_right: Option<String>,

    // ---- assets ----
    /// Path (relative to the db file) of the full card crop PNG.
    pub card_image_path: String,
    /// Path of the extracted center illustration PNG, if one was found.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub illustration_path: Option<String>,

    // ---- lossless OCR ----
    /// Every OCR'd text line with position/size/confidence.
    #[serde(rename = "raw_line", skip_serializing_if = "Vec::is_empty")]
    pub raw_lines: Vec<RawLine>,
}

/// One OCR'd text line (a group of words sharing a Tesseract line).
#[derive(Debug, Clone, Serialize)]
pub struct RawLine {
    pub text: String,
    /// Bounding box in card-crop pixels.
    pub bbox: BBox,
    /// Median word height in the line (a glyph-size proxy).
    pub glyph_height: u32,
    /// Mean word confidence [0,100].
    pub confidence: f32,
    pub block: u32,
    pub par: u32,
    pub line: u32,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct BBox {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl BBox {
    pub fn center_x(&self) -> f32 {
        self.x as f32 + self.w as f32 / 2.0
    }
    pub fn center_y(&self) -> f32 {
        self.y as f32 + self.h as f32 / 2.0
    }
    pub fn bottom(&self) -> u32 {
        self.y + self.h
    }
    pub fn right(&self) -> u32 {
        self.x + self.w
    }
}
