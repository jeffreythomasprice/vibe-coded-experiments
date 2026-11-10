use ab_glyph::{Font, FontRef, PxScale, ScaleFont, point};
use epaint_default_fonts::HACK_REGULAR;

const SIZE_PX: f32 = 14.0;
const PAD: i32 = 2;

pub struct Text {
    pub text: String,
    pub x: f32,
    pub y: f32,
    /// The point the label belongs to; a leader line joins it to a label that was moved away.
    pub anchor: Option<(f32, f32)>,
}

/// Size in pixels of the box `draw_labels` draws for `text`.
pub fn text_size(text: &str) -> (i32, i32) {
    let Ok(font) = FontRef::try_from_slice(HACK_REGULAR) else {
        return (0, 0);
    };
    let scaled = font.as_scaled(PxScale::from(SIZE_PX));
    let width: f32 = text
        .chars()
        .map(|c| scaled.h_advance(scaled.glyph_id(c)))
        .sum();
    (
        width.ceil() as i32 + 2 * PAD,
        scaled.height().ceil() as i32 + 2 * PAD,
    )
}

fn blend(px: &mut [u8], rgb: [u8; 3], alpha: f32) {
    for (dst, src) in px.iter_mut().zip(rgb) {
        *dst = (f32::from(*dst) * (1.0 - alpha) + f32::from(src) * alpha).round() as u8;
    }
}

fn leader(rgba: &mut [u8], w: i32, h: i32, from: (f32, f32), to: (f32, f32)) {
    let steps = (to.0 - from.0)
        .abs()
        .max((to.1 - from.1).abs())
        .ceil()
        .max(1.0) as i32;
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let x = (from.0 + (to.0 - from.0) * t).round() as i32;
        let y = (from.1 + (to.1 - from.1) * t).round() as i32;
        let size = if i == 0 { 1 } else { 0 };
        for dy in -size..=size {
            for dx in -size..=size {
                let (px, py) = (x + dx, y + dy);
                if (0..w).contains(&px) && (0..h).contains(&py) {
                    let i = ((py * w + px) * 4) as usize;
                    blend(&mut rgba[i..i + 3], [255, 235, 90], 1.0);
                }
            }
        }
    }
}

/// Draws each text centred on `(x, y)` over a dark box, clamped inside the image.
pub fn draw_labels(rgba: &mut [u8], width: u32, height: u32, labels: &[Text]) {
    let Ok(font) = FontRef::try_from_slice(HACK_REGULAR) else {
        return;
    };
    let scaled = font.as_scaled(PxScale::from(SIZE_PX));
    let ascent = scaled.ascent();
    let line_height = scaled.height();
    let (w, h) = (width as i32, height as i32);

    for label in labels {
        if let Some((ax, ay)) = label.anchor {
            leader(rgba, w, h, (ax, ay), (label.x, label.y));
        }
    }

    for label in labels {
        let text_width: f32 = label
            .text
            .chars()
            .map(|c| scaled.h_advance(scaled.glyph_id(c)))
            .sum();
        let box_w = text_width.ceil() as i32 + 2 * PAD;
        let box_h = line_height.ceil() as i32 + 2 * PAD;
        let x0 = (label.x as i32 - box_w / 2).clamp(0, (w - box_w).max(0));
        let y0 = (label.y as i32 - box_h / 2).clamp(0, (h - box_h).max(0));

        for y in y0.max(0)..(y0 + box_h).min(h) {
            for x in x0.max(0)..(x0 + box_w).min(w) {
                let i = ((y * w + x) * 4) as usize;
                blend(&mut rgba[i..i + 3], [0, 0, 0], 0.7);
            }
        }

        let mut pen = (x0 + PAD) as f32;
        for c in label.text.chars() {
            let glyph = scaled.scaled_glyph(c);
            let advance = scaled.h_advance(glyph.id);
            let mut glyph = glyph;
            glyph.position = point(pen, (y0 + PAD) as f32 + ascent);
            pen += advance;
            if let Some(outlined) = font.outline_glyph(glyph) {
                let bounds = outlined.px_bounds();
                outlined.draw(|gx, gy, coverage| {
                    let x = bounds.min.x as i32 + gx as i32;
                    let y = bounds.min.y as i32 + gy as i32;
                    if (0..w).contains(&x) && (0..h).contains(&y) {
                        let i = ((y * w + x) * 4) as usize;
                        blend(&mut rgba[i..i + 3], [255, 255, 255], coverage);
                    }
                });
            }
        }
    }
}
