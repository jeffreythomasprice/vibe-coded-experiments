use glam::Vec2;
use image::DynamicImage;
use std::collections::HashMap;
use std::sync::Arc;

use crate::gfx::Texture;

#[derive(Clone, Copy)]
pub struct UvRect {
    pub min: Vec2,
    pub max: Vec2,
}

pub struct TextureAtlas {
    texture: Arc<Texture>,
    rects: HashMap<String, UvRect>,
    sizes: HashMap<String, (u32, u32)>,
}

impl TextureAtlas {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        images: HashMap<String, DynamicImage>,
    ) -> Self {
        let mut entries: Vec<(String, DynamicImage)> = images.into_iter().collect();
        entries.sort_by(|a, b| b.1.height().cmp(&a.1.height()));

        let total_area: u64 = entries
            .iter()
            .map(|(_, img)| img.width() as u64 * img.height() as u64)
            .sum();
        let max_w = entries.iter().map(|(_, img)| img.width()).max().unwrap_or(1);
        let atlas_width = max_w.max((total_area as f64).sqrt().ceil() as u32).max(1);

        struct Placement {
            name: String,
            x: u32,
            y: u32,
            w: u32,
            h: u32,
        }

        let mut placements: Vec<Placement> = Vec::with_capacity(entries.len());
        let mut cursor_x: u32 = 0;
        let mut cursor_y: u32 = 0;
        let mut shelf_h: u32 = 0;

        for (name, img) in &entries {
            let (w, h) = (img.width(), img.height());
            if cursor_x + w > atlas_width {
                cursor_y += shelf_h;
                cursor_x = 0;
                shelf_h = 0;
            }
            placements.push(Placement { name: name.clone(), x: cursor_x, y: cursor_y, w, h });
            cursor_x += w;
            shelf_h = shelf_h.max(h);
        }

        let atlas_height = (cursor_y + shelf_h).max(1);
        let mut pixels = vec![0u8; (atlas_width * atlas_height * 4) as usize];

        let img_map: HashMap<&str, &DynamicImage> =
            entries.iter().map(|(k, v)| (k.as_str(), v)).collect();

        for p in &placements {
            let rgba = img_map[p.name.as_str()].to_rgba8();
            let raw = rgba.as_raw();
            for row in 0..p.h {
                let src = (row * p.w * 4) as usize;
                let dst = ((p.y + row) * atlas_width + p.x) as usize * 4;
                pixels[dst..dst + (p.w * 4) as usize]
                    .copy_from_slice(&raw[src..src + (p.w * 4) as usize]);
            }
        }

        let texture = Arc::new(
            Texture::new_with_image_bytes(device, queue, atlas_width, atlas_height, &pixels)
        );

        let (tw, th) = (atlas_width as f32, atlas_height as f32);
        let rects = placements
            .iter()
            .map(|p| {
                let uv = UvRect {
                    min: Vec2::new(p.x as f32 / tw, p.y as f32 / th),
                    max: Vec2::new((p.x + p.w) as f32 / tw, (p.y + p.h) as f32 / th),
                };
                (p.name.clone(), uv)
            })
            .collect();

        let sizes = placements.iter().map(|p| (p.name.clone(), (p.w, p.h))).collect();

        Self { texture, rects, sizes }
    }

    pub fn texture(&self) -> &Texture {
        &self.texture
    }

    pub fn texture_arc(&self) -> Arc<Texture> {
        Arc::clone(&self.texture)
    }

    pub fn get(&self, name: &str) -> Option<UvRect> {
        self.rects.get(name).copied()
    }

    pub fn size(&self, name: &str) -> Option<(u32, u32)> {
        self.sizes.get(name).copied()
    }
}
