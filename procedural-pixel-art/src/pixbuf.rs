//! A small buffer of (material, shade) pixels with the drawing operations parts need.

use crate::palette::{Material, Palette};
use crate::stamp::Stamp;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pixel {
    pub material: Material,
    pub shade: u8,
}

impl Pixel {
    pub const OUTLINE: Self = Self {
        material: Material::Outline,
        shade: 0,
    };
}

#[derive(Clone, Debug)]
pub struct PixBuf {
    pub width: i32,
    pub height: i32,
    pixels: Vec<Option<Pixel>>,
}

impl PixBuf {
    pub fn new(width: i32, height: i32) -> Self {
        Self {
            width,
            height,
            pixels: vec![None; (width * height) as usize],
        }
    }

    pub fn get(&self, x: i32, y: i32) -> Option<Pixel> {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return None;
        }
        self.pixels[(y * self.width + x) as usize]
    }

    /// Writes outside the buffer are clipped.
    pub fn set(&mut self, x: i32, y: i32, pixel: Pixel) {
        if x >= 0 && y >= 0 && x < self.width && y < self.height {
            self.pixels[(y * self.width + x) as usize] = Some(pixel);
        }
    }

    fn coords(&self) -> impl Iterator<Item = (i32, i32)> + use<> {
        let (w, h) = (self.width, self.height);
        (0..h).flat_map(move |y| (0..w).map(move |x| (x, y)))
    }

    /// Fills the ellipse inscribed in the box with top-left (`left`, `top`).
    pub fn fill_ellipse(&mut self, left: i32, top: i32, w: i32, h: i32, material: Material) {
        let (rx, ry) = (f64::from(w) / 2.0, f64::from(h) / 2.0);
        for y in 0..h {
            for x in 0..w {
                let dx = (f64::from(x) + 0.5 - rx) / rx;
                let dy = (f64::from(y) + 0.5 - ry) / ry;
                if dx * dx + dy * dy <= 1.0 {
                    self.set(left + x, top + y, Pixel { material, shade: 2 });
                }
            }
        }
    }

    /// Fills a box with its four corner pixels removed.
    pub fn fill_rounded_rect(&mut self, left: i32, top: i32, w: i32, h: i32, material: Material) {
        for y in 0..h {
            for x in 0..w {
                let corner = (x == 0 || x == w - 1) && (y == 0 || y == h - 1);
                if !corner {
                    self.set(left + x, top + y, Pixel { material, shade: 2 });
                }
            }
        }
    }

    pub fn blit_stamp(&mut self, stamp: &Stamp, center_x: i32, center_y: i32, flip_x: bool) {
        for y in 0..stamp.height {
            for x in 0..stamp.width {
                let Some(pixel) = stamp.get(x, y) else {
                    continue;
                };
                let dx = x - stamp.anchor.0;
                // The anchor is a pixel *boundary*, so mirroring maps dx to -dx - 1.
                let dx = if flip_x { -dx - 1 } else { dx };
                self.set(center_x + dx, center_y + y - stamp.anchor.1, pixel);
            }
        }
    }

    /// Lights shapes from above: a darker rim along the bottom, a highlight along the top.
    pub fn shade_pass(&mut self) {
        let source = self.clone();
        for (x, y) in self.coords() {
            let Some(pixel) = source.get(x, y) else {
                continue;
            };
            let shade = if source.get(x, y + 1).is_none() {
                1
            } else if source.get(x, y - 1).is_none() {
                3
            } else {
                continue;
            };
            self.set(x, y, Pixel { shade, ..pixel });
        }
    }

    /// Drops every pixel one shade, to push a part into the background.
    pub fn darken(&mut self) {
        for pixel in self.pixels.iter_mut().flatten() {
            pixel.shade = pixel.shade.saturating_sub(1);
        }
    }

    /// Surrounds the art with an outline. Existing outline pixels do not grow a
    /// second one, so stamps may carry their own.
    pub fn outline_pass(&mut self) {
        let source = self.clone();
        for (x, y) in self.coords() {
            if source.get(x, y).is_some() {
                continue;
            }
            let touches_art = [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(dx, dy)| {
                source
                    .get(x + dx, y + dy)
                    .is_some_and(|p| p.material != Material::Outline)
            });
            if touches_art {
                self.set(x, y, Pixel::OUTLINE);
            }
        }
    }

    /// Draws `layer`, which must be the same size, on top of this buffer.
    pub fn composite(&mut self, layer: &PixBuf) {
        for (dst, src) in self.pixels.iter_mut().zip(&layer.pixels) {
            if src.is_some() {
                *dst = *src;
            }
        }
    }

    /// Resolves colours into an RGBA frame with top-left at (`left`, `top`).
    pub fn paint(
        &self,
        frame: &mut [u8],
        frame_width: i32,
        left: i32,
        top: i32,
        palette: &Palette,
    ) {
        let frame_height = frame.len() as i32 / 4 / frame_width;
        for (x, y) in self.coords() {
            let (fx, fy) = (left + x, top + y);
            if fx < 0 || fy < 0 || fx >= frame_width || fy >= frame_height {
                continue;
            }
            if let Some(p) = self.get(x, y) {
                let i = ((fy * frame_width + fx) * 4) as usize;
                frame[i..i + 3].copy_from_slice(&palette.color(p.material, p.shade));
                frame[i + 3] = 255;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SKIN: Pixel = Pixel {
        material: Material::Skin,
        shade: 2,
    };

    #[test]
    fn outline_surrounds_a_pixel_without_diagonals() {
        let mut buf = PixBuf::new(3, 3);
        buf.set(1, 1, SKIN);
        buf.outline_pass();
        assert_eq!(buf.get(1, 0), Some(Pixel::OUTLINE));
        assert_eq!(buf.get(0, 1), Some(Pixel::OUTLINE));
        assert_eq!(buf.get(0, 0), None);
        assert_eq!(buf.get(1, 1), Some(SKIN));
    }

    #[test]
    fn outline_does_not_grow_from_outline() {
        let mut buf = PixBuf::new(3, 1);
        buf.set(1, 0, Pixel::OUTLINE);
        buf.outline_pass();
        assert_eq!(buf.get(0, 0), None);
    }

    #[test]
    fn drawing_clips_at_the_edges() {
        let mut buf = PixBuf::new(2, 2);
        buf.fill_ellipse(-3, -3, 10, 10, Material::Skin);
        assert!(buf.get(0, 0).is_some());
        assert!(buf.get(2, 0).is_none());
    }
}
