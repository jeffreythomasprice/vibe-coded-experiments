use std::collections::HashMap;

use super::wgpu_utils::Texture;

pub struct AtlasRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

pub struct AtlasUv {
    pub u0: f32,
    pub v0: f32,
    pub u1: f32,
    pub v1: f32,
}

pub struct AtlasEntry {
    pub pixel_rect: AtlasRect,
    pub uv: AtlasUv,
}

pub struct AtlasImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub struct TextureAtlas {
    pub texture: Texture,
    pub width: u32,
    pub height: u32,
    entries: HashMap<String, AtlasEntry>,
}

impl TextureAtlas {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        images: HashMap<String, AtlasImage>,
    ) -> Self {
        let mut items: Vec<(String, AtlasImage)> = images
            .into_iter()
            .filter(|(_, img)| img.width > 0 && img.height > 0)
            .collect();
        items.sort_by(|a, b| b.1.height.cmp(&a.1.height).then(b.1.width.cmp(&a.1.width)));

        let atlas_width: u32 = 1024;
        let mut cursor_x: u32 = 0;
        let mut cursor_y: u32 = 0;
        let mut row_height: u32 = 0;

        let mut placements: Vec<(String, u32, u32, AtlasImage)> = Vec::with_capacity(items.len());

        for (name, img) in items {
            if cursor_x + img.width > atlas_width {
                cursor_y += row_height + 1;
                cursor_x = 0;
                row_height = 0;
            }
            placements.push((name, cursor_x, cursor_y, img));
            cursor_x += placements.last().unwrap().3.width + 1;
            row_height = row_height.max(placements.last().unwrap().3.height);
        }

        let atlas_height = if placements.is_empty() {
            1
        } else {
            cursor_y + row_height
        };

        let mut atlas_data = vec![0u8; (atlas_width * atlas_height * 4) as usize];

        let mut entries = HashMap::new();
        let w_f = atlas_width as f32;
        let h_f = atlas_height as f32;

        for (name, px, py, img) in &placements {
            for row in 0..img.height {
                let src_offset = (row * img.width * 4) as usize;
                let dst_offset = ((py + row) * atlas_width * 4 + px * 4) as usize;
                let row_bytes = (img.width * 4) as usize;
                atlas_data[dst_offset..dst_offset + row_bytes]
                    .copy_from_slice(&img.rgba[src_offset..src_offset + row_bytes]);
            }

            entries.insert(
                name.clone(),
                AtlasEntry {
                    pixel_rect: AtlasRect {
                        x: *px,
                        y: *py,
                        w: img.width,
                        h: img.height,
                    },
                    uv: AtlasUv {
                        u0: *px as f32 / w_f,
                        v0: *py as f32 / h_f,
                        u1: (*px + img.width) as f32 / w_f,
                        v1: (*py + img.height) as f32 / h_f,
                    },
                },
            );
        }

        let texture =
            Texture::from_rgba(device, queue, atlas_width, atlas_height, &atlas_data, "texture_atlas");

        Self {
            texture,
            width: atlas_width,
            height: atlas_height,
            entries,
        }
    }

    pub fn get(&self, name: &str) -> Option<&AtlasEntry> {
        self.entries.get(name)
    }
}
