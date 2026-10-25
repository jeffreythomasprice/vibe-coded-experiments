use std::collections::HashMap;

use ab_glyph::{Font, GlyphId, PxScale, point};

use crate::text::{Fonts, Weight};

const SIZE: u32 = 2048;
const PAD: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Key {
    weight: Weight,
    id: u16,
    px_tenths: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct Entry {
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
    pub offset: [f32; 2],
    pub size: [f32; 2],
}

pub struct Atlas {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    entries: HashMap<Key, Option<Entry>>,
    cursor: (u32, u32),
    row_height: u32,
    warned_full: bool,
}

impl Atlas {
    pub fn new(device: &wgpu::Device) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("glyph atlas"),
            size: wgpu::Extent3d {
                width: SIZE,
                height: SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Atlas {
            texture,
            view,
            entries: HashMap::new(),
            cursor: (0, 0),
            row_height: 0,
            warned_full: false,
        }
    }

    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    pub fn glyph(
        &mut self,
        queue: &wgpu::Queue,
        fonts: &Fonts,
        weight: Weight,
        id: GlyphId,
        px: f32,
    ) -> Option<Entry> {
        let key = Key {
            weight,
            id: id.0,
            px_tenths: (px * 10.0).round() as u32,
        };
        if let Some(entry) = self.entries.get(&key) {
            return *entry;
        }
        let entry = self.rasterize(queue, fonts, weight, id, px);
        self.entries.insert(key, entry);
        entry
    }

    fn rasterize(
        &mut self,
        queue: &wgpu::Queue,
        fonts: &Fonts,
        weight: Weight,
        id: GlyphId,
        px: f32,
    ) -> Option<Entry> {
        let glyph = id.with_scale_and_position(PxScale::from(px), point(0.0, 0.0));
        let outlined = fonts.font(weight).outline_glyph(glyph)?;
        let bounds = outlined.px_bounds();
        let (w, h) = (bounds.width().ceil() as u32, bounds.height().ceil() as u32);
        if w == 0 || h == 0 {
            return None;
        }
        let mut coverage = vec![0u8; (w * h) as usize];
        outlined.draw(|x, y, c| {
            if x < w && y < h {
                coverage[(y * w + x) as usize] = (c.clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        });
        let (x, y) = self.allocate(w + 2 * PAD, h + 2 * PAD)?;
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: x + PAD,
                    y: y + PAD,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &coverage,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        let atlas = SIZE as f32;
        let (x0, y0) = ((x + PAD) as f32, (y + PAD) as f32);
        Some(Entry {
            uv_min: [x0 / atlas, y0 / atlas],
            uv_max: [(x0 + w as f32) / atlas, (y0 + h as f32) / atlas],
            offset: [bounds.min.x, bounds.min.y],
            size: [w as f32, h as f32],
        })
    }

    fn allocate(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        if self.cursor.0 + w > SIZE {
            self.cursor = (0, self.cursor.1 + self.row_height);
            self.row_height = 0;
        }
        if self.cursor.1 + h > SIZE {
            if !self.warned_full {
                log::warn!("glyph atlas is full");
                self.warned_full = true;
            }
            return None;
        }
        let at = self.cursor;
        self.cursor.0 += w;
        self.row_height = self.row_height.max(h);
        Some(at)
    }
}
