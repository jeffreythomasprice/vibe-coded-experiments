//! A tiny 5x7 bitmap font. `fill_text` antialiases, which blurs at pixel art
//! resolutions, so glyphs are plotted as individual pixels instead.

use web_sys::CanvasRenderingContext2d;

const GLYPH_WIDTH: u32 = 5;
const ADVANCE: u32 = GLYPH_WIDTH + 1;

/// Rows top to bottom; bit 4 is the leftmost pixel. Unknown characters are blank.
fn glyph(c: char) -> [u8; 7] {
    match c {
        'D' => [
            0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110,
        ],
        'E' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
        ],
        'H' => [
            0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'L' => [
            0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
        ],
        'O' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'R' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ],
        'W' => [
            0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010,
        ],
        'S' => [
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        '0' => [
            0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
        ],
        '1' => [
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        '2' => [
            0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
        ],
        '3' => [
            0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        '4' => [
            0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
        ],
        '5' => [
            0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110,
        ],
        '6' => [
            0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
        ],
        '7' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
        ],
        '8' => [
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ],
        '9' => [
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100,
        ],
        _ => [0; 7],
    }
}

/// Width of `text` in font pixels, excluding trailing spacing.
pub fn text_width(text: &str) -> u32 {
    (text.chars().count() as u32 * ADVANCE).saturating_sub(1)
}

/// Draws `text` with its top-left corner at (`x`, `y`) in the current fill style.
/// Each font pixel is a `size` by `size` square.
pub fn draw_text(ctx: &CanvasRenderingContext2d, text: &str, x: f64, y: f64, size: f64) {
    for (i, c) in text.chars().enumerate() {
        let left = x + f64::from(i as u32 * ADVANCE) * size;
        for (row, bits) in glyph(c).into_iter().enumerate() {
            for col in 0..GLYPH_WIDTH {
                if bits & (1 << (GLYPH_WIDTH - 1 - col)) != 0 {
                    let px = left + f64::from(col) * size;
                    let py = y + row as f64 * size;
                    ctx.fill_rect(px, py, size, size);
                }
            }
        }
    }
}
