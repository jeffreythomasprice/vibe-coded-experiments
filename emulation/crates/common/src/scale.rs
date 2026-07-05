//! Aspect-ratio / scale modes and the placement math that turns a source frame
//! size plus a surface size into a centered destination rectangle, expressed as a
//! vertex [`Transform`].
//!
//! This lives in `common` (not the `video` crate) so it is both the runtime type
//! the renderer consumes and the value persisted in settings — a single enum, no
//! duplication. It is deliberately free of any `wgpu` dependency; the letterbox
//! fill color, which needs `wgpu::Color`, stays in the `video` crate.

use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

/// How the source frame is sized to fit the surface.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScaleMode {
    /// One source pixel per one physical pixel, centered.
    Original,
    /// Preserve aspect ratio, multiplying the source size by a constant factor.
    Scaled(f32),
    /// Preserve aspect ratio, scaled as large as possible to fit within the surface.
    #[default]
    Fit,
    /// Ignore aspect ratio, stretch to fill the whole surface.
    Stretch,
}

/// Per-frame vertex uniform: how to place the full-NDC unit quad. Because the quad
/// spans `[-1, 1]` symmetrically about the origin, a centered destination needs only
/// a scale (fraction of each axis the frame covers) with a zero offset.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct Transform {
    pub scale: [f32; 2],
    pub offset: [f32; 2],
}

impl ScaleMode {
    /// Destination size in physical pixels for the given source and surface sizes,
    /// rounded to whole pixels to avoid sub-pixel seams under nearest sampling.
    pub fn dest_size(self, src_w: u32, src_h: u32, surf_w: u32, surf_h: u32) -> (u32, u32) {
        let (sw, sh) = (src_w as f32, src_h as f32);
        let (fw, fh) = (surf_w as f32, surf_h as f32);
        let (dw, dh) = match self {
            ScaleMode::Original => (sw, sh),
            ScaleMode::Scaled(factor) => (sw * factor, sh * factor),
            ScaleMode::Fit => {
                let s = (fw / sw).min(fh / sh);
                (sw * s, sh * s)
            }
            ScaleMode::Stretch => (fw, fh),
        };
        (dw.round().max(0.0) as u32, dh.round().max(0.0) as u32)
    }

    /// Vertex [`Transform`] placing the centered destination rectangle within the surface.
    pub fn transform(self, src_w: u32, src_h: u32, surf_w: u32, surf_h: u32) -> Transform {
        let (dw, dh) = self.dest_size(src_w, src_h, surf_w, surf_h);
        let scale_x = if surf_w == 0 { 0.0 } else { dw as f32 / surf_w as f32 };
        let scale_y = if surf_h == 0 { 0.0 } else { dh as f32 / surf_h as f32 };
        Transform {
            scale: [scale_x, scale_y],
            offset: [0.0, 0.0],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn original_is_one_to_one() {
        assert_eq!(ScaleMode::Original.dest_size(64, 48, 800, 600), (64, 48));
    }

    #[test]
    fn scaled_multiplies_both_axes() {
        assert_eq!(ScaleMode::Scaled(2.5).dest_size(64, 48, 800, 600), (160, 120));
    }

    #[test]
    fn fit_maximizes_preserving_aspect() {
        // Square source into a 200x100 surface: height-limited -> 100x100, centered.
        assert_eq!(ScaleMode::Fit.dest_size(64, 64, 200, 100), (100, 100));
        // Wider surface than source aspect: width-limited.
        assert_eq!(ScaleMode::Fit.dest_size(160, 144, 640, 576), (640, 576));
    }

    #[test]
    fn stretch_fills_the_surface() {
        assert_eq!(ScaleMode::Stretch.dest_size(64, 64, 200, 100), (200, 100));
    }

    #[test]
    fn transform_scale_is_dest_fraction_with_zero_offset() {
        let t = ScaleMode::Fit.transform(64, 64, 200, 100);
        assert_eq!(t.offset, [0.0, 0.0]);
        assert_eq!(t.scale, [100.0 / 200.0, 100.0 / 100.0]);
    }

    #[test]
    fn stretch_transform_covers_full_ndc() {
        let t = ScaleMode::Stretch.transform(64, 64, 200, 100);
        assert_eq!(t.scale, [1.0, 1.0]);
    }

    #[test]
    fn scale_mode_round_trips_through_toml() {
        // Sanity: the enum used in settings serializes cleanly both ways.
        for mode in [
            ScaleMode::Original,
            ScaleMode::Fit,
            ScaleMode::Stretch,
            ScaleMode::Scaled(4.0),
        ] {
            let s = toml::to_string(&Wrap { mode }).unwrap();
            let back: Wrap = toml::from_str(&s).unwrap();
            assert_eq!(back.mode, mode);
        }
    }

    #[derive(Serialize, Deserialize)]
    struct Wrap {
        mode: ScaleMode,
    }
}
