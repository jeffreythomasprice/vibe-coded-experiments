//! Within a single card crop: locate the center illustration and classify the
//! OCR'd text lines into title / top-extras / body / bottom corners.

use crate::config::LayoutConfig;
use crate::imageops::{background_luma, to_gray};
use crate::model::{BBox, RawLine};
use image::RgbImage;

#[derive(Debug, Default)]
pub struct Analysis {
    pub title: Option<String>,
    pub top_extras: Vec<String>,
    pub body: Option<String>,
    pub bottom_left: Option<String>,
    pub bottom_right: Option<String>,
    /// Bounding box of the center illustration within the crop, if found.
    pub illustration: Option<BBox>,
}

pub fn analyze(crop: &RgbImage, lines: &[RawLine], cfg: &LayoutConfig) -> Analysis {
    let (w, h) = crop.dimensions();
    let illustration = find_illustration(crop, lines, cfg);
    let (img_top, img_bottom) = match &illustration {
        Some(b) => (b.y as f32, b.bottom() as f32),
        None => (h as f32 / 2.0, h as f32 / 2.0),
    };

    let bottom_cut = h as f32 * (1.0 - cfg.bottom_band_frac);
    let left_third = w as f32 / 3.0;
    let right_third = w as f32 * 2.0 / 3.0;
    // Lines this high are treated as the title area even if they fall inside a
    // dense illustration band (bold titles read as ink).
    let title_zone = h as f32 * 0.20;

    let mut bottom_left: Vec<&RawLine> = Vec::new();
    let mut bottom_right: Vec<&RawLine> = Vec::new();
    let mut content: Vec<&RawLine> = Vec::new();

    for l in lines {
        if is_garbage(l) {
            continue;
        }
        let cy = l.bbox.center_y();

        // Drop OCR noise picked out of the cartoon. Keep confident multi-word
        // lines (a banner like "GO UP A LEVEL") and anything in the title zone.
        let in_band = illustration.is_some() && cy > img_top && cy < img_bottom;
        if in_band && cy > title_zone && !is_real_text(l) {
            continue;
        }

        // Bottom-corner detection takes precedence. A corner label can be wide
        // enough that its center sits inside the middle third (e.g. "400 Gold
        // Pieces"), so classify by which corner third the line's edges reach
        // into rather than by its center. A line reaching exactly one corner is
        // assigned there; one reaching neither (centered) or both (full-width)
        // falls through to the body.
        if l.bbox.bottom() as f32 >= bottom_cut {
            let reaches_left = (l.bbox.x as f32) < left_third;
            let reaches_right = (l.bbox.right() as f32) > right_third;
            if reaches_left && !reaches_right {
                bottom_left.push(l);
                continue;
            }
            if reaches_right && !reaches_left {
                bottom_right.push(l);
                continue;
            }
        }
        content.push(l);
    }

    let (title, top_extras, body) = classify(&content, h);

    Analysis {
        title,
        top_extras,
        body,
        bottom_left: stitch(&bottom_left),
        bottom_right: stitch(&bottom_right),
        illustration,
    }
}

/// A line is likely OCR garbage if it is one stray glyph with low confidence.
fn is_garbage(l: &RawLine) -> bool {
    let alnum = l.text.chars().filter(|c| c.is_alphanumeric()).count();
    alnum <= 1 && l.confidence < 70.0
}

/// A confident multi-word line (real text even if inside the illustration band).
fn is_real_text(l: &RawLine) -> bool {
    let words = l.text.split_whitespace().count();
    let alnum = l.text.chars().filter(|c| c.is_alphanumeric()).count();
    l.confidence >= 80.0 && words >= 2 && alnum >= 5
}

/// Locate the center illustration as the tallest run of ink-dense rows that are
/// *not* occupied by OCR'd text (small gaps merged). Excluding text rows keeps
/// the title, body, and corner text out of the illustration band; the geometric
/// density picks up the cartoon between them.
fn find_illustration(crop: &RgbImage, lines: &[RawLine], cfg: &LayoutConfig) -> Option<BBox> {
    let (w, h) = crop.dimensions();
    let gray = to_gray(crop);
    let bg = background_luma(&gray) as i32;
    let dark_thresh = (bg - 35).max(0) as u8;

    let inset_x = (w as f32 * 0.06) as u32;
    let inset_y = (h as f32 * 0.04) as u32;
    let x_lo = inset_x;
    let x_hi = w.saturating_sub(inset_x);
    let interior_w = x_hi.saturating_sub(x_lo).max(1);

    let mut ink_frac = vec![0f32; h as usize];
    for y in 0..h {
        let mut cnt = 0u32;
        for x in x_lo..x_hi {
            if gray.get_pixel(x, y)[0] < dark_thresh {
                cnt += 1;
            }
        }
        ink_frac[y as usize] = cnt as f32 / interior_w as f32;
    }

    // Mark rows overlapped by *confident* OCR text so they can't be part of the
    // illustration band. We use a high confidence floor so the hallucinated
    // glyphs Tesseract paints onto the cartoon (low confidence) don't split the
    // figure, while real title/body/corner text (high confidence) is excluded.
    let mut text_row = vec![false; h as usize];
    for l in lines {
        if l.confidence < 75.0 {
            continue;
        }
        let y0 = l.bbox.y.min(h.saturating_sub(1));
        let y1 = l.bbox.bottom().min(h);
        for y in y0..y1 {
            text_row[y as usize] = true;
        }
    }

    // Merge gaps up to ~1.2% of card height (smaller than inter-line spacing).
    let merge_gap = ((h as f32) * 0.012).max(4.0) as u32;

    let mut best: Option<(u32, u32)> = None;
    let mut run_start: Option<u32> = None;
    let mut last_dense: u32 = 0;
    let top = inset_y;
    let bot = h.saturating_sub(inset_y);
    for y in top..bot {
        let dense = ink_frac[y as usize] >= cfg.ink_row_frac && !text_row[y as usize];
        if dense {
            if run_start.is_none() {
                run_start = Some(y);
            }
            last_dense = y;
        } else if let Some(s) = run_start {
            if y - last_dense > merge_gap {
                update_best(&mut best, s, last_dense + 1);
                run_start = None;
            }
        }
    }
    if let Some(s) = run_start {
        update_best(&mut best, s, last_dense + 1);
    }

    let (band_top, band_bottom) = best?;
    if (band_bottom - band_top) < (h as f32 * cfg.min_illu_frac) as u32 {
        return None;
    }

    // Trim horizontally to the ink extent within the band.
    let mut x_min = w;
    let mut x_max = 0u32;
    for y in band_top..band_bottom {
        for x in x_lo..x_hi {
            if gray.get_pixel(x, y)[0] < dark_thresh {
                x_min = x_min.min(x);
                x_max = x_max.max(x);
            }
        }
    }
    if x_min > x_max {
        return None;
    }
    Some(BBox {
        x: x_min,
        y: band_top,
        w: x_max - x_min + 1,
        h: band_bottom - band_top,
    })
}

fn update_best(best: &mut Option<(u32, u32)>, start: u32, end: u32) {
    let len = end - start;
    if best.map_or(true, |(s, e)| len > (e - s)) {
        *best = Some((start, end));
    }
}

/// Classify content lines (everything that isn't a bottom corner) into the
/// title, the smaller lines above it (`top_extras`), and the body below it.
///
/// The title is anchored as the tallest text in the upper half of the card;
/// `top_extras`/`body` are then split by position relative to the title rather
/// than relative to the illustration (whose location varies). Wrapped lines are
/// stitched within each region.
fn classify(content: &[&RawLine], h: u32) -> (Option<String>, Vec<String>, Option<String>) {
    if content.is_empty() {
        return (None, Vec::new(), None);
    }

    // The title and its sub-labels live in the header (top third). The body is
    // the remaining text. The title is the tallest line(s) within the header.
    let header_cut = h as f32 * 0.32;
    let header: Vec<&RawLine> = content
        .iter()
        .copied()
        .filter(|l| l.bbox.center_y() < header_cut)
        .collect();

    if header.is_empty() {
        // No header text (e.g. the stylized name wasn't OCR'd) — everything is
        // body, no title.
        return (None, Vec::new(), stitch(content));
    }

    let max_h = header.iter().map(|l| l.glyph_height).max().unwrap_or(0);
    let cutoff = max_h as f32 * 0.8;

    let mut title_lines: Vec<&RawLine> = Vec::new();
    let mut top_lines: Vec<&RawLine> = Vec::new();
    for l in &header {
        if l.glyph_height as f32 >= cutoff {
            title_lines.push(l);
        } else {
            top_lines.push(l);
        }
    }

    let title = stitch(&title_lines);
    let top_extras = top_lines
        .iter()
        .filter_map(|l| {
            let t = l.text.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        })
        .collect();

    let body_lines: Vec<&RawLine> = content
        .iter()
        .copied()
        .filter(|l| l.bbox.center_y() >= header_cut)
        .collect();
    let body = stitch(&body_lines);

    (title, top_extras, body)
}

/// Join lines top-to-bottom, stitching wrapped text. Inserts a paragraph break
/// (newline) when the vertical gap is large or the glyph size changes notably.
fn stitch(lines: &[&RawLine]) -> Option<String> {
    if lines.is_empty() {
        return None;
    }
    let mut sorted: Vec<&RawLine> = lines.to_vec();
    sorted.sort_by_key(|l| l.bbox.y);

    let mut out = String::new();
    let mut prev: Option<&RawLine> = None;
    for l in &sorted {
        let text = l.text.trim();
        if text.is_empty() {
            continue;
        }
        if let Some(p) = prev {
            let gap = l.bbox.y as i32 - p.bbox.bottom() as i32;
            let line_h = p.glyph_height.max(1) as i32;
            let size_change =
                (l.glyph_height as f32 - p.glyph_height as f32).abs() / p.glyph_height.max(1) as f32;
            if gap > line_h * 3 / 2 || size_change > 0.4 {
                out.push('\n');
            } else {
                out.push(' ');
            }
        }
        out.push_str(text);
        prev = Some(l);
    }
    let out = out.trim().to_string();
    if out.is_empty() { None } else { Some(out) }
}
