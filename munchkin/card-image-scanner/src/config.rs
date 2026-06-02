//! Tunable configuration, built from CLI args.

use crate::cli::{Args, SegmentMode};

#[derive(Debug, Clone)]
pub struct SegmentConfig {
    pub mode: SegmentMode,
    pub bin_threshold: Option<u8>,
    pub morph_radius: u32,
    pub aspect_min: f32,
    pub aspect_max: f32,
    pub min_area_frac: f32,
    pub max_area_frac: f32,
    pub crop_pad: u32,
    pub rows: Option<u32>,
    pub cols: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct OcrConfig {
    pub bin: String,
    pub lang: String,
    pub psm: u32,
    pub min_conf: f32,
}

#[derive(Debug, Clone)]
pub struct LayoutConfig {
    /// Fraction of card height treated as the bottom-corner band.
    pub bottom_band_frac: f32,
    /// Minimum illustration height as a fraction of card height.
    pub min_illu_frac: f32,
    /// A row counts as "ink" if this fraction of its interior pixels are
    /// non-background.
    pub ink_row_frac: f32,
}

impl SegmentConfig {
    pub fn from_args(a: &Args) -> Self {
        Self {
            mode: a.segment_mode,
            bin_threshold: a.bin_threshold,
            morph_radius: a.morph_radius,
            aspect_min: a.aspect_min,
            aspect_max: a.aspect_max,
            min_area_frac: a.min_area_frac,
            max_area_frac: a.max_area_frac,
            crop_pad: a.crop_pad,
            rows: a.rows,
            cols: a.cols,
        }
    }
}

impl OcrConfig {
    pub fn from_args(a: &Args) -> Self {
        Self {
            bin: a.tesseract_bin.clone(),
            lang: a.tesseract_lang.clone(),
            psm: a.tess_psm,
            min_conf: a.min_conf,
        }
    }
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            bottom_band_frac: 0.15,
            min_illu_frac: 0.12,
            ink_row_frac: 0.04,
        }
    }
}
