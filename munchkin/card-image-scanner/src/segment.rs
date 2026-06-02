//! Segment a full card sheet into individual, reading-order-ordered cards.

use crate::cli::SegmentMode;
use crate::config::SegmentConfig;
use crate::imageops::binarize;
use crate::model::BBox;
use image::{GrayImage, Luma, RgbImage};
use imageproc::contrast::otsu_level;
use imageproc::morphology::close;
use imageproc::region_labelling::{Connectivity, connected_components};
use imageproc::distance_transform::Norm;

/// Result of segmenting a sheet: ordered card boxes plus the binarized mask
/// (returned for debug visualization).
pub struct Segmentation {
    pub boxes: Vec<BBox>,
    /// Binarization threshold used (Otsu or the manual override).
    pub threshold: u8,
    /// The binarized border mask (for --debug visualization).
    pub binarized: GrayImage,
}

pub fn detect_cards(sheet: &RgbImage, cfg: &SegmentConfig) -> Segmentation {
    let gray = image::imageops::grayscale(sheet);
    let threshold = cfg.bin_threshold.unwrap_or_else(|| otsu_level(&gray));
    let ink = binarize(&gray, threshold); // 255 = dark border/ink

    // Bridge small gaps in the dark border so each card border is a closed ring.
    let sealed = if cfg.morph_radius > 0 {
        close(&ink, Norm::LInf, cfg.morph_radius as u8)
    } else {
        ink.clone()
    };

    // Hard grid override short-circuits detection.
    if let (Some(rows), Some(cols)) = (cfg.rows, cfg.cols) {
        return Segmentation {
            boxes: grid_split(sheet.width(), sheet.height(), rows, cols, cfg.crop_pad),
            threshold,
            binarized: ink,
        };
    }

    let mut boxes = match cfg.mode {
        SegmentMode::Projection => projection_boxes(&sealed, cfg),
        _ => component_boxes(&sealed, cfg),
    };

    // Auto fallback to projection if component detection found nothing usable.
    if boxes.is_empty() && cfg.mode == SegmentMode::Auto {
        boxes = projection_boxes(&sealed, cfg);
    }

    // Single-card guard: nothing detected but the whole sheet is card-shaped.
    if boxes.is_empty() {
        let (w, h) = sheet.dimensions();
        let aspect = w as f32 / h as f32;
        if aspect >= cfg.aspect_min && aspect <= cfg.aspect_max {
            boxes.push(BBox { x: 0, y: 0, w, h });
        }
    }

    let boxes = order_reading(boxes);

    Segmentation {
        boxes,
        threshold,
        binarized: ink,
    }
}

/// Detect cards from the dark border rings. Each card's border is one connected
/// component (separated from neighbors by clean white gutters); its bounding box
/// is the card. This is robust to small border gaps that would let a bright
/// interior leak into the background.
fn component_boxes(sealed: &GrayImage, cfg: &SegmentConfig) -> Vec<BBox> {
    // `sealed` is 255 = dark border/ink, 0 = background. Label the dark regions.
    let labels = connected_components(sealed, Connectivity::Eight, Luma([0u8]));

    // Accumulate per-label bounding boxes.
    let mut min_x: Vec<u32> = Vec::new();
    let mut min_y: Vec<u32> = Vec::new();
    let mut max_x: Vec<u32> = Vec::new();
    let mut max_y: Vec<u32> = Vec::new();
    let ensure = |v: &mut Vec<u32>, n: usize, fill: u32| {
        if v.len() <= n {
            v.resize(n + 1, fill);
        }
    };
    for (x, y, p) in labels.enumerate_pixels() {
        let l = p[0] as usize;
        if l == 0 {
            continue;
        }
        ensure(&mut min_x, l, u32::MAX);
        ensure(&mut min_y, l, u32::MAX);
        ensure(&mut max_x, l, 0);
        ensure(&mut max_y, l, 0);
        min_x[l] = min_x[l].min(x);
        min_y[l] = min_y[l].min(y);
        max_x[l] = max_x[l].max(x);
        max_y[l] = max_y[l].max(y);
    }

    let sheet_area = (sealed.width() as f32) * (sealed.height() as f32);
    let mut out = Vec::new();
    for l in 1..min_x.len() {
        if min_x[l] == u32::MAX {
            continue;
        }
        let w = max_x[l] - min_x[l] + 1;
        let h = max_y[l] - min_y[l] + 1;
        let area = (w as f32) * (h as f32);
        let frac = area / sheet_area;
        let aspect = w as f32 / h as f32;
        if frac < cfg.min_area_frac || frac > cfg.max_area_frac {
            continue;
        }
        if aspect < cfg.aspect_min || aspect > cfg.aspect_max {
            continue;
        }
        // The component bounds trace the OUTER edge of the dark border ring.
        // Inset inward past the frame so the crop is the clean cream interior,
        // which OCRs far better than a crop that includes the dark border.
        let inset_x = (w as f32 * 0.04) as u32;
        let inset_y = (h as f32 * 0.03) as u32;
        let x = min_x[l] + inset_x;
        let y = min_y[l] + inset_y;
        let x1 = (max_x[l] + 1).saturating_sub(inset_x);
        let y1 = (max_y[l] + 1).saturating_sub(inset_y);
        out.push(BBox {
            x,
            y,
            w: x1 - x,
            h: y1 - y,
        });
    }
    suppress_contained(out)
}

/// Drop boxes contained within (or heavily overlapping) a larger box. The card
/// border ring is the largest box in its region; inner illustration/text
/// components nest inside it and are suppressed.
fn suppress_contained(mut boxes: Vec<BBox>) -> Vec<BBox> {
    boxes.sort_by(|a, b| (b.w * b.h).cmp(&(a.w * a.h)));
    let mut kept: Vec<BBox> = Vec::new();
    for b in boxes {
        let cx = b.center_x();
        let cy = b.center_y();
        let inside = kept.iter().any(|k| {
            cx >= k.x as f32
                && cx <= k.right() as f32
                && cy >= k.y as f32
                && cy <= k.bottom() as f32
        });
        if !inside {
            kept.push(b);
        }
    }
    kept
}

/// Projection-profile fallback: find white gutters in row/column ink profiles.
fn projection_boxes(sealed: &GrayImage, cfg: &SegmentConfig) -> Vec<BBox> {
    let (w, h) = sealed.dimensions();
    let col_ink = axis_profile(sealed, true);
    let row_ink = axis_profile(sealed, false);
    let col_bands = content_bands(&col_ink, w);
    let row_bands = content_bands(&row_ink, h);

    let sheet_area = w as f32 * h as f32;
    let mut out = Vec::new();
    for &(ry, rh) in &row_bands {
        for &(cx, cw) in &col_bands {
            let area = cw as f32 * rh as f32;
            let frac = area / sheet_area;
            let aspect = cw as f32 / rh as f32;
            if frac < cfg.min_area_frac || frac > cfg.max_area_frac {
                continue;
            }
            if aspect < cfg.aspect_min || aspect > cfg.aspect_max {
                continue;
            }
            out.push(BBox {
                x: cx,
                y: ry,
                w: cw,
                h: rh,
            });
        }
    }
    out
}

/// Sum of ink pixels per column (vertical=true) or per row.
fn axis_profile(sealed: &GrayImage, vertical: bool) -> Vec<u32> {
    let (w, h) = sealed.dimensions();
    let n = if vertical { w } else { h } as usize;
    let mut prof = vec![0u32; n];
    for (x, y, p) in sealed.enumerate_pixels() {
        if p[0] > 0 {
            let idx = if vertical { x } else { y } as usize;
            prof[idx] += 1;
        }
    }
    prof
}

/// Identify contiguous "content" bands separated by low-ink gutters.
fn content_bands(prof: &[u32], _len: u32) -> Vec<(u32, u32)> {
    let max = *prof.iter().max().unwrap_or(&0) as f32;
    if max == 0.0 {
        return Vec::new();
    }
    // A position is "gutter" if its ink is below 5% of the peak.
    let cutoff = (max * 0.05) as u32;
    let mut bands = Vec::new();
    let mut start: Option<usize> = None;
    for (i, &v) in prof.iter().enumerate() {
        if v > cutoff {
            if start.is_none() {
                start = Some(i);
            }
        } else if let Some(s) = start.take() {
            bands.push((s as u32, (i - s) as u32));
        }
    }
    if let Some(s) = start {
        bands.push((s as u32, (prof.len() - s) as u32));
    }
    // Drop tiny bands (noise): keep those at least 10% of the largest band.
    let max_band = bands.iter().map(|b| b.1).max().unwrap_or(0) as f32;
    bands
        .into_iter()
        .filter(|b| b.1 as f32 >= max_band * 0.3)
        .collect()
}

/// Split the sheet into an even rows×cols grid (hard override).
fn grid_split(w: u32, h: u32, rows: u32, cols: u32, pad: u32) -> Vec<BBox> {
    let cw = w / cols;
    let ch = h / rows;
    let mut out = Vec::new();
    for r in 0..rows {
        for c in 0..cols {
            let x = (c * cw).saturating_sub(pad);
            let y = (r * ch).saturating_sub(pad);
            let x1 = ((c + 1) * cw + pad).min(w);
            let y1 = ((r + 1) * ch + pad).min(h);
            out.push(BBox {
                x,
                y,
                w: x1 - x,
                h: y1 - y,
            });
        }
    }
    out
}

/// Order boxes row-major, clustering into rows by center-y to tolerate skew.
fn order_reading(mut boxes: Vec<BBox>) -> Vec<BBox> {
    if boxes.is_empty() {
        return boxes;
    }
    let median_h = {
        let mut hs: Vec<u32> = boxes.iter().map(|b| b.h).collect();
        hs.sort_unstable();
        hs[hs.len() / 2]
    };
    let tol = median_h as f32 * 0.5;

    boxes.sort_by(|a, b| a.center_y().partial_cmp(&b.center_y()).unwrap());

    let mut rows: Vec<Vec<BBox>> = Vec::new();
    for b in boxes {
        if let Some(row) = rows.last_mut() {
            let row_cy = row.iter().map(|r| r.center_y()).sum::<f32>() / row.len() as f32;
            if (b.center_y() - row_cy).abs() <= tol {
                row.push(b);
                continue;
            }
        }
        rows.push(vec![b]);
    }

    let mut out = Vec::new();
    for mut row in rows {
        row.sort_by(|a, b| a.center_x().partial_cmp(&b.center_x()).unwrap());
        out.extend(row);
    }
    out
}
