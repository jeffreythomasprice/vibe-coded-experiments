//! Source pixel formats and conversion to the tightly-packed RGBA8 the GPU wants.
//!
//! Emulator video memory comes in many shapes: interleaved byte channels
//! (RGB8/RGBA8/BGR8/BGRA8) or sub-byte packed indices into a palette (1/2/4 bits
//! per pixel). [`to_rgba8`] normalizes all of them into one RGBA8 buffer that
//! [`crate::PixelBufferRenderer`] uploads to a texture.

/// An 8-bit-per-channel RGBA color, used both for palette entries and conversion output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const BLACK: Color = Color { r: 0, g: 0, b: 0, a: 255 };
    pub const WHITE: Color = Color { r: 255, g: 255, b: 255, a: 255 };

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Color { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Color { r, g, b, a }
    }

    fn write_to(self, out: &mut Vec<u8>) {
        out.extend_from_slice(&[self.r, self.g, self.b, self.a]);
    }
}

/// Within a packed byte, which sub-pixel is the leftmost on screen.
///
/// `MsbFirst` means the highest-order bits hold the leftmost pixel (the common
/// convention, e.g. Game Boy tile data); `LsbFirst` is the reverse.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BitOrder {
    #[default]
    MsbFirst,
    LsbFirst,
}

/// How the bytes of a [`PixelBuffer`] are encoded.
///
/// The interleaved variants carry no extra data. The packed variants each carry
/// a palette sized to their bit depth (2/4/16 entries) plus a [`BitOrder`].
#[derive(Clone, Debug)]
pub enum PixelFormat {
    /// 3 bytes per pixel, red-green-blue; alpha is forced opaque.
    Rgb8,
    /// 4 bytes per pixel, red-green-blue-alpha.
    Rgba8,
    /// 3 bytes per pixel, blue-green-red; alpha is forced opaque.
    Bgr8,
    /// 4 bytes per pixel, blue-green-red-alpha.
    Bgra8,
    /// 1 bit per pixel, 8 pixels per byte, indexing a 2-color palette.
    Mono1 { palette: [Color; 2], order: BitOrder },
    /// 2 bits per pixel, 4 pixels per byte, indexing a 4-color palette.
    Grey2 { palette: [Color; 4], order: BitOrder },
    /// 4 bits per pixel, 2 pixels per byte, indexing a 16-color palette.
    Grey4 { palette: [Color; 16], order: BitOrder },
}

/// A borrowed source frame: raw bytes plus the geometry and encoding needed to
/// interpret them. `stride` is the number of bytes between the start of one row
/// and the next, allowing padded rows.
pub struct PixelBuffer<'a> {
    pub data: &'a [u8],
    pub width: u32,
    pub height: u32,
    pub stride: usize,
    pub format: PixelFormat,
}

#[derive(Debug, thiserror::Error)]
pub enum FormatError {
    #[error("row stride {stride} is smaller than the minimum {needed} bytes for a row of {width} pixels")]
    StrideTooSmall { stride: usize, needed: usize, width: u32 },
    #[error("buffer holds {actual} bytes but {needed} are required for {width}x{height} at stride {stride}")]
    BufferTooSmall {
        actual: usize,
        needed: usize,
        width: u32,
        height: u32,
        stride: usize,
    },
}

/// Bits per pixel for a packed format, or `None` for the interleaved byte formats.
fn packed_bits(format: &PixelFormat) -> Option<u32> {
    match format {
        PixelFormat::Mono1 { .. } => Some(1),
        PixelFormat::Grey2 { .. } => Some(2),
        PixelFormat::Grey4 { .. } => Some(4),
        _ => None,
    }
}

/// Minimum number of bytes a single row of `width` pixels occupies in `format`.
fn min_row_bytes(format: &PixelFormat, width: u32) -> usize {
    let w = width as usize;
    match format {
        PixelFormat::Rgb8 | PixelFormat::Bgr8 => w * 3,
        PixelFormat::Rgba8 | PixelFormat::Bgra8 => w * 4,
        _ => {
            let bits = packed_bits(format).unwrap();
            let px_per_byte = (8 / bits) as usize;
            w.div_ceil(px_per_byte)
        }
    }
}

/// Convert any supported source format into tightly-packed RGBA8 (`width * height * 4`
/// bytes) written into `out`. `out` is cleared first and reused for its capacity.
pub fn to_rgba8(buf: &PixelBuffer, out: &mut Vec<u8>) -> Result<(), FormatError> {
    let row_bytes = min_row_bytes(&buf.format, buf.width);
    if buf.stride < row_bytes {
        return Err(FormatError::StrideTooSmall {
            stride: buf.stride,
            needed: row_bytes,
            width: buf.width,
        });
    }

    // The last row only needs `row_bytes`, not a full stride of trailing padding.
    let needed = if buf.height == 0 {
        0
    } else {
        (buf.height as usize - 1) * buf.stride + row_bytes
    };
    if buf.data.len() < needed {
        return Err(FormatError::BufferTooSmall {
            actual: buf.data.len(),
            needed,
            width: buf.width,
            height: buf.height,
            stride: buf.stride,
        });
    }

    out.clear();
    out.reserve(buf.width as usize * buf.height as usize * 4);

    for y in 0..buf.height as usize {
        let row = &buf.data[y * buf.stride..];
        match &buf.format {
            PixelFormat::Rgb8 => convert_interleaved(row, buf.width, 3, [0, 1, 2], out),
            PixelFormat::Bgr8 => convert_interleaved(row, buf.width, 3, [2, 1, 0], out),
            PixelFormat::Rgba8 => convert_interleaved(row, buf.width, 4, [0, 1, 2], out),
            PixelFormat::Bgra8 => convert_interleaved(row, buf.width, 4, [2, 1, 0], out),
            PixelFormat::Mono1 { palette, order } => {
                convert_packed(row, buf.width, 1, *order, palette, out)
            }
            PixelFormat::Grey2 { palette, order } => {
                convert_packed(row, buf.width, 2, *order, palette, out)
            }
            PixelFormat::Grey4 { palette, order } => {
                convert_packed(row, buf.width, 4, *order, palette, out)
            }
        }
    }

    Ok(())
}

/// Convert one interleaved-byte row. `rgb_idx` maps output R/G/B to source byte
/// offsets (so BGR simply passes `[2, 1, 0]`). With `bpp == 4` the fourth source
/// byte is alpha; with `bpp == 3` alpha is forced opaque.
fn convert_interleaved(row: &[u8], width: u32, bpp: usize, rgb_idx: [usize; 3], out: &mut Vec<u8>) {
    for x in 0..width as usize {
        let px = &row[x * bpp..x * bpp + bpp];
        let a = if bpp == 4 { px[3] } else { 255 };
        out.extend_from_slice(&[px[rgb_idx[0]], px[rgb_idx[1]], px[rgb_idx[2]], a]);
    }
}

/// Convert one sub-byte packed row, looking each pixel's index up in `palette`.
fn convert_packed(
    row: &[u8],
    width: u32,
    bits: u32,
    order: BitOrder,
    palette: &[Color],
    out: &mut Vec<u8>,
) {
    let px_per_byte = 8 / bits;
    let mask = (1u8 << bits) - 1;
    for x in 0..width {
        let byte = row[(x / px_per_byte) as usize];
        let sub = x % px_per_byte; // 0 = first pixel in the byte, left to right
        // MsbFirst: pixel 0 sits in the highest-order bits.
        let shift = match order {
            BitOrder::MsbFirst => (px_per_byte - 1 - sub) * bits,
            BitOrder::LsbFirst => sub * bits,
        };
        let index = ((byte >> shift) & mask) as usize;
        palette[index].write_to(out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgba(buf: &PixelBuffer) -> Vec<u8> {
        let mut out = Vec::new();
        to_rgba8(buf, &mut out).unwrap();
        out
    }

    #[test]
    fn rgb8_forces_opaque_alpha() {
        let data = [10, 20, 30, 40, 50, 60];
        let out = rgba(&PixelBuffer {
            data: &data,
            width: 2,
            height: 1,
            stride: 6,
            format: PixelFormat::Rgb8,
        });
        assert_eq!(out, vec![10, 20, 30, 255, 40, 50, 60, 255]);
    }

    #[test]
    fn bgr8_and_bgra8_swap_red_and_blue() {
        let bgr = [1, 2, 3];
        assert_eq!(
            rgba(&PixelBuffer { data: &bgr, width: 1, height: 1, stride: 3, format: PixelFormat::Bgr8 }),
            vec![3, 2, 1, 255]
        );
        let bgra = [1, 2, 3, 128];
        assert_eq!(
            rgba(&PixelBuffer { data: &bgra, width: 1, height: 1, stride: 4, format: PixelFormat::Bgra8 }),
            vec![3, 2, 1, 128]
        );
    }

    #[test]
    fn stride_padding_is_skipped() {
        // Two rows of one RGBA pixel each, with 4 padding bytes after each row.
        let data = [1, 2, 3, 4, 0, 0, 0, 0, 5, 6, 7, 8, 0, 0, 0, 0];
        let out = rgba(&PixelBuffer {
            data: &data,
            width: 1,
            height: 2,
            stride: 8,
            format: PixelFormat::Rgba8,
        });
        assert_eq!(out, vec![1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn grey2_msb_first_reads_high_bits_leftmost() {
        let palette = [Color::rgb(0, 0, 0), Color::rgb(85, 85, 85), Color::rgb(170, 170, 170), Color::WHITE];
        // 0b00_01_10_11 -> indices 0,1,2,3 left to right.
        let data = [0b00_01_10_11];
        let out = rgba(&PixelBuffer {
            data: &data,
            width: 4,
            height: 1,
            stride: 1,
            format: PixelFormat::Grey2 { palette, order: BitOrder::MsbFirst },
        });
        assert_eq!(
            out,
            vec![0, 0, 0, 255, 85, 85, 85, 255, 170, 170, 170, 255, 255, 255, 255, 255]
        );
    }

    #[test]
    fn grey2_lsb_first_reverses_within_byte() {
        let palette = [Color::rgb(0, 0, 0), Color::rgb(85, 85, 85), Color::rgb(170, 170, 170), Color::WHITE];
        // Same byte as above but LSB-first: pixel 0 = low bits = 0b11 = index 3.
        let data = [0b00_01_10_11];
        let out = rgba(&PixelBuffer {
            data: &data,
            width: 4,
            height: 1,
            stride: 1,
            format: PixelFormat::Grey2 { palette, order: BitOrder::LsbFirst },
        });
        assert_eq!(
            out,
            vec![255, 255, 255, 255, 170, 170, 170, 255, 85, 85, 85, 255, 0, 0, 0, 255]
        );
    }

    #[test]
    fn mono1_expands_eight_pixels_per_byte() {
        let palette = [Color::BLACK, Color::WHITE];
        let data = [0b1010_0001];
        let out = rgba(&PixelBuffer {
            data: &data,
            width: 8,
            height: 1,
            stride: 1,
            format: PixelFormat::Mono1 { palette, order: BitOrder::MsbFirst },
        });
        let bits = [1, 0, 1, 0, 0, 0, 0, 1];
        let expected: Vec<u8> = bits
            .iter()
            .flat_map(|&b| if b == 1 { [255, 255, 255, 255] } else { [0, 0, 0, 255] })
            .collect();
        assert_eq!(out, expected);
    }

    #[test]
    fn grey4_two_pixels_per_byte() {
        let mut palette = [Color::BLACK; 16];
        for (i, c) in palette.iter_mut().enumerate() {
            let v = (i * 17) as u8; // 0..=255 across 16 steps
            *c = Color::rgb(v, v, v);
        }
        // 0xA3 -> high nibble 0xA=10, low nibble 0x3=3 (MSB-first: 10 then 3).
        let data = [0xA3];
        let out = rgba(&PixelBuffer {
            data: &data,
            width: 2,
            height: 1,
            stride: 1,
            format: PixelFormat::Grey4 { palette, order: BitOrder::MsbFirst },
        });
        assert_eq!(out, vec![170, 170, 170, 255, 51, 51, 51, 255]);
    }

    #[test]
    fn stride_too_small_is_rejected() {
        let data = [0u8; 4];
        let err = to_rgba8(
            &PixelBuffer { data: &data, width: 4, height: 1, stride: 1, format: PixelFormat::Rgba8 },
            &mut Vec::new(),
        );
        assert!(matches!(err, Err(FormatError::StrideTooSmall { .. })));
    }

    #[test]
    fn buffer_too_small_is_rejected() {
        let data = [0u8; 3];
        let err = to_rgba8(
            &PixelBuffer { data: &data, width: 1, height: 2, stride: 4, format: PixelFormat::Rgba8 },
            &mut Vec::new(),
        );
        assert!(matches!(err, Err(FormatError::BufferTooSmall { .. })));
    }
}
