use std::collections::HashMap;

use anyhow::{bail, Result};

use crate::overlay::{Rgba, Texture};

pub struct AtlasRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

pub struct TextureAtlas {
    texture: Texture,
    regions: HashMap<String, AtlasRegion>,
}

impl TextureAtlas {
    pub fn region(&self, name: &str) -> Option<&AtlasRegion> {
        self.regions.get(name)
    }

    pub fn uv_rect(&self, name: &str) -> Option<([f32; 2], [f32; 2])> {
        let r = self.regions.get(name)?;
        let tw = self.texture.width() as f32;
        let th = self.texture.height() as f32;
        Some((
            [r.x as f32 / tw, r.y as f32 / th],
            [(r.x + r.width) as f32 / tw, (r.y + r.height) as f32 / th],
        ))
    }

    pub fn texture(&self) -> &Texture {
        &self.texture
    }
}

pub struct TextureAtlasBuilder {
    entries: Vec<(String, Texture)>,
}

impl TextureAtlasBuilder {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn add(&mut self, name: impl Into<String>, image: Texture) {
        self.entries.push((name.into(), image));
    }

    pub fn build(mut self) -> Result<TextureAtlas> {
        if self.entries.is_empty() {
            return Ok(TextureAtlas {
                texture: Texture::new(1, 1),
                regions: HashMap::new(),
            });
        }

        self.entries
            .sort_by(|a, b| b.1.height().cmp(&a.1.height()));

        let mut atlas_size = 256u32;
        let max_size = 4096u32;

        loop {
            if let Some(result) = try_pack(&self.entries, atlas_size) {
                return Ok(result);
            }
            atlas_size *= 2;
            if atlas_size > max_size {
                bail!("atlas entries exceed maximum atlas size of {max_size}x{max_size}");
            }
        }
    }
}

fn try_pack(entries: &[(String, Texture)], atlas_size: u32) -> Option<TextureAtlas> {
    const PADDING: u32 = 1;

    let mut atlas = Texture::new(atlas_size, atlas_size);
    let mut regions = HashMap::new();

    let mut cursor_x = 0u32;
    let mut cursor_y = 0u32;
    let mut row_height = 0u32;

    for (name, img) in entries {
        if cursor_x + img.width() > atlas_size {
            cursor_x = 0;
            cursor_y += row_height + PADDING;
            row_height = 0;
        }
        if cursor_y + img.height() > atlas_size {
            return None;
        }

        for py in 0..img.height() {
            for px in 0..img.width() {
                atlas.set_pixel(cursor_x + px, cursor_y + py, img.get_pixel(px, py));
            }
        }

        regions.insert(
            name.clone(),
            AtlasRegion {
                x: cursor_x,
                y: cursor_y,
                width: img.width(),
                height: img.height(),
            },
        );

        cursor_x += img.width() + PADDING;
        if img.height() > row_height {
            row_height = img.height();
        }
    }

    Some(TextureAtlas {
        texture: atlas,
        regions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_image_packs_correctly() {
        let mut builder = TextureAtlasBuilder::new();
        let mut img = Texture::new(16, 16);
        img.set_pixel(0, 0, Rgba::WHITE);
        builder.add("test", img);
        let atlas = builder.build().unwrap();
        let region = atlas.region("test").unwrap();
        assert_eq!(region.x, 0);
        assert_eq!(region.y, 0);
        assert_eq!(region.width, 16);
        assert_eq!(region.height, 16);
    }

    #[test]
    fn uv_rect_returns_normalized_coords() {
        let mut builder = TextureAtlasBuilder::new();
        builder.add("test", Texture::new(128, 128));
        let atlas = builder.build().unwrap();
        let (uv_min, uv_max) = atlas.uv_rect("test").unwrap();
        assert_eq!(uv_min, [0.0, 0.0]);
        assert_eq!(uv_max, [128.0 / 256.0, 128.0 / 256.0]);
    }

    #[test]
    fn multiple_images_pack_without_overlap() {
        let mut builder = TextureAtlasBuilder::new();
        builder.add("a", Texture::new(64, 32));
        builder.add("b", Texture::new(32, 64));
        builder.add("c", Texture::new(100, 50));
        let atlas = builder.build().unwrap();

        let regions: Vec<&AtlasRegion> = ["a", "b", "c"]
            .iter()
            .map(|n| atlas.region(n).unwrap())
            .collect();

        for i in 0..regions.len() {
            for j in (i + 1)..regions.len() {
                let a = regions[i];
                let b = regions[j];
                let no_overlap = a.x + a.width <= b.x
                    || b.x + b.width <= a.x
                    || a.y + a.height <= b.y
                    || b.y + b.height <= a.y;
                assert!(no_overlap, "regions {i} and {j} overlap");
            }
        }
    }

    #[test]
    fn nonexistent_name_returns_none() {
        let builder = TextureAtlasBuilder::new();
        let atlas = builder.build().unwrap();
        assert!(atlas.region("nope").is_none());
        assert!(atlas.uv_rect("nope").is_none());
    }

    #[test]
    fn empty_builder_produces_empty_atlas() {
        let builder = TextureAtlasBuilder::new();
        let atlas = builder.build().unwrap();
        assert!(atlas.regions.is_empty());
    }
}
