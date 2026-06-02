//! Debug visualizations (only used under --debug).

use crate::layout::Analysis;
use crate::model::{BBox, RawLine};
use image::{Rgb, RgbImage};
use imageproc::drawing::draw_hollow_rect_mut;
use imageproc::rect::Rect;

const RED: Rgb<u8> = Rgb([220, 30, 30]);
const GREEN: Rgb<u8> = Rgb([30, 160, 30]);
const BLUE: Rgb<u8> = Rgb([30, 80, 220]);
const ORANGE: Rgb<u8> = Rgb([230, 140, 0]);
const PURPLE: Rgb<u8> = Rgb([150, 30, 200]);

fn rect(b: &BBox) -> Option<Rect> {
    if b.w == 0 || b.h == 0 {
        return None;
    }
    Some(Rect::at(b.x as i32, b.y as i32).of_size(b.w, b.h))
}

/// Draw detected card boxes (with a thick red outline) on a copy of the sheet.
pub fn sheet_overlay(sheet: &RgbImage, boxes: &[BBox]) -> RgbImage {
    let mut img = sheet.clone();
    for b in boxes {
        // Draw a few nested rects for a thicker, more visible line.
        for d in 0..6u32 {
            let inner = BBox {
                x: b.x + d,
                y: b.y + d,
                w: b.w.saturating_sub(2 * d),
                h: b.h.saturating_sub(2 * d),
            };
            if let Some(r) = rect(&inner) {
                draw_hollow_rect_mut(&mut img, r, RED);
            }
        }
    }
    img
}

/// Draw the analyzed regions and raw OCR line boxes on a copy of the card crop.
pub fn card_overlay(crop: &RgbImage, analysis: &Analysis, lines: &[RawLine]) -> RgbImage {
    let mut img = crop.clone();
    // Raw OCR lines in faint blue.
    for l in lines {
        if let Some(r) = rect(&l.bbox) {
            draw_hollow_rect_mut(&mut img, r, BLUE);
        }
    }
    // Illustration band in green (doubled for thickness).
    if let Some(b) = &analysis.illustration {
        for d in 0..3u32 {
            let inner = BBox {
                x: b.x + d,
                y: b.y + d,
                w: b.w.saturating_sub(2 * d),
                h: b.h.saturating_sub(2 * d),
            };
            if let Some(r) = rect(&inner) {
                draw_hollow_rect_mut(&mut img, r, GREEN);
            }
        }
    }
    // Bottom-corner bands (orange/purple) marked along the bottom edge.
    let (w, h) = crop.dimensions();
    let band_h = (h as f32 * 0.15) as u32;
    let third = w / 3;
    if analysis.bottom_left.is_some() {
        if let Some(r) = rect(&BBox {
            x: 0,
            y: h.saturating_sub(band_h),
            w: third,
            h: band_h,
        }) {
            draw_hollow_rect_mut(&mut img, r, ORANGE);
        }
    }
    if analysis.bottom_right.is_some() {
        if let Some(r) = rect(&BBox {
            x: 2 * third,
            y: h.saturating_sub(band_h),
            w: third,
            h: band_h,
        }) {
            draw_hollow_rect_mut(&mut img, r, PURPLE);
        }
    }
    img
}
