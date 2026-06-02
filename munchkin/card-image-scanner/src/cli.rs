//! Command-line interface.

use clap::{Parser, ValueEnum};

#[derive(Parser, Debug)]
#[command(
    name = "card-image-scanner",
    about = "Segment scanned Munchkin card sheets, OCR each card, and extract \
             the center illustration into a TOML database + PNG files."
)]
pub struct Args {
    /// Image files or glob patterns (e.g. ../rules-and-card-scans/door*.jpg).
    #[arg(required = true)]
    pub inputs: Vec<String>,

    // ---- output ----
    /// Directory for PNG crops/illustrations.
    #[arg(long, default_value = "./out")]
    pub out_dir: String,

    /// Aggregate TOML database path [default: <out-dir>/cards.toml].
    #[arg(long, visible_alias = "out-toml")]
    pub db: Option<String>,

    // ---- data ----
    /// Path to Munchkin-CardList.txt for title correction. If omitted, looks
    /// for it next to the first input image.
    #[arg(long)]
    pub card_list: Option<String>,

    /// Card type; `auto` infers from the filename prefix (door/loot).
    #[arg(long, value_enum, default_value_t = CardType::Auto)]
    pub r#type: CardType,

    // ---- OCR ----
    #[arg(long, default_value = "tesseract")]
    pub tesseract_bin: String,
    #[arg(long, default_value = "eng")]
    pub tesseract_lang: String,
    /// Tesseract page-segmentation mode (--psm).
    #[arg(long, default_value_t = 11)]
    pub tess_psm: u32,
    /// Drop OCR words below this confidence (0-100).
    #[arg(long, default_value_t = 30.0)]
    pub min_conf: f32,
    /// Fuzzy-match cutoff [0,1] for correcting titles against the card list.
    #[arg(long, default_value_t = 0.82)]
    pub title_match_threshold: f64,

    // ---- segmentation ----
    #[arg(long, value_enum, default_value_t = SegmentMode::Auto)]
    pub segment_mode: SegmentMode,
    /// Manual binarization threshold (0-255). Omit to use Otsu.
    #[arg(long)]
    pub bin_threshold: Option<u8>,
    /// Morphological close radius (px) to bridge border gaps.
    #[arg(long, default_value_t = 3)]
    pub morph_radius: u32,
    #[arg(long, default_value_t = 0.55)]
    pub aspect_min: f32,
    #[arg(long, default_value_t = 0.78)]
    pub aspect_max: f32,
    /// Minimum card area as a fraction of the whole sheet area.
    #[arg(long, default_value_t = 0.02)]
    pub min_area_frac: f32,
    /// Maximum card area as a fraction of the whole sheet area.
    #[arg(long, default_value_t = 0.6)]
    pub max_area_frac: f32,
    /// Pad each detected card crop by this many pixels.
    #[arg(long, default_value_t = 8)]
    pub crop_pad: u32,
    /// Hard grid override: number of rows (use with --cols).
    #[arg(long)]
    pub rows: Option<u32>,
    /// Hard grid override: number of columns (use with --rows).
    #[arg(long)]
    pub cols: Option<u32>,

    // ---- debug ----
    /// Save intermediate images (binarized sheet, annotated crops).
    #[arg(long)]
    pub debug: bool,
    /// Where to write debug images [default: <out-dir>/debug].
    #[arg(long)]
    pub debug_dir: Option<String>,
    #[arg(short, long)]
    pub verbose: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum CardType {
    Door,
    Loot,
    Auto,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum SegmentMode {
    Components,
    Projection,
    Auto,
}
