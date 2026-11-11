use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::{Arc, Mutex};

use editor_core::{Material, TextureSource};

use editor_cmd::texture::{decode_rgba8, load};

/// Largest side of a texture the renderer uploads; bigger images are scaled down.
pub const MAX_SIDE: u32 = 4096;
/// Side of the texture shown instead of an image that cannot be loaded.
const CHECKER_SIDE: u32 = 8;

/// A decoded image ready for upload.
pub struct TextureData {
    pub key: u64,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// The size of the image as stored, before any scaling to `MAX_SIDE`.
    pub original: (u32, u32),
}

/// What the viewport needs to draw faces of one material.
pub struct RenderMaterial {
    pub id: editor_core::MaterialId,
    pub texture: Option<Arc<TextureData>>,
    pub tint: [f32; 4],
    pub emissive: [f32; 3],
    pub nearest: bool,
    pub unlit: bool,
    pub blended: bool,
}

impl RenderMaterial {
    pub fn size(&self) -> (f64, f64) {
        match &self.texture {
            Some(t) => (f64::from(t.original.0), f64::from(t.original.1)),
            None => (editor_geom::uv::NOMINAL_SIZE, editor_geom::uv::NOMINAL_SIZE),
        }
    }
}

const NODRAW_ALPHA: f32 = 0.35;

/// Decoded images shared between rebuilds of the scene.
#[derive(Default)]
pub struct TextureCache {
    map: Mutex<HashMap<String, Arc<TextureData>>>,
}

fn hash_of(text: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

fn checker() -> Arc<TextureData> {
    let mut rgba = Vec::new();
    for y in 0..CHECKER_SIDE {
        for x in 0..CHECKER_SIDE {
            let on = (x / 2 + y / 2) % 2 == 0;
            rgba.extend_from_slice(if on {
                &[255, 0, 255, 255]
            } else {
                &[24, 24, 24, 255]
            });
        }
    }
    Arc::new(TextureData {
        key: hash_of("editor-render:checker"),
        width: CHECKER_SIDE,
        height: CHECKER_SIDE,
        rgba,
        original: (CHECKER_SIDE, CHECKER_SIDE),
    })
}

impl TextureCache {
    /// The decoded image of `source`, or the reason it cannot be used.
    pub fn get(
        &self,
        source: &TextureSource,
        scene_dir: Option<&Path>,
    ) -> Result<Arc<TextureData>, String> {
        let loaded = load(source, scene_dir).map_err(|e| e.to_string())?;
        let key = match (&loaded.file, loaded.modified) {
            (Some(file), modified) => {
                let nanos = modified
                    .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_nanos());
                format!("file|{}|{nanos}|{}", file.display(), loaded.bytes.len())
            }
            (None, _) => {
                let mut h = std::collections::hash_map::DefaultHasher::new();
                loaded.bytes.hash(&mut h);
                format!("embedded|{:016x}|{}", h.finish(), loaded.bytes.len())
            }
        };
        if let Some(hit) = self.map.lock().expect("texture cache").get(&key) {
            return Ok(hit.clone());
        }
        let (size, mut rgba) =
            decode_rgba8(&loaded.bytes, loaded.format).map_err(|e| e.to_string())?;
        let original = (size.width, size.height);
        let (mut width, mut height) = original;
        if width.max(height) > MAX_SIDE {
            let scale = f64::from(MAX_SIDE) / f64::from(width.max(height));
            let (nw, nh) = (
                ((f64::from(width) * scale).round() as u32).max(1),
                ((f64::from(height) * scale).round() as u32).max(1),
            );
            let img = image::RgbaImage::from_raw(width, height, rgba)
                .ok_or_else(|| "decoded image has the wrong size".to_string())?;
            rgba = image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Triangle)
                .into_raw();
            (width, height) = (nw, nh);
        }
        let data = Arc::new(TextureData {
            key: hash_of(&key),
            width,
            height,
            rgba,
            original,
        });
        self.map
            .lock()
            .expect("texture cache")
            .insert(key, data.clone());
        Ok(data)
    }

    /// The render description of `material`. A broken image becomes a checker and a message in `problem`.
    pub fn material(
        &self,
        material: &Material,
        scene_dir: Option<&Path>,
        problem: &mut Option<String>,
    ) -> RenderMaterial {
        let texture = material.texture.as_ref().map(|source| {
            self.get(source, scene_dir).unwrap_or_else(|e| {
                *problem = Some(format!("material {:?}: {e}", material.name));
                checker()
            })
        });
        let mut tint = material.color;
        if material.nodraw {
            tint[3] = tint[3].min(NODRAW_ALPHA);
        }
        RenderMaterial {
            id: material.id,
            texture,
            tint,
            emissive: material.emissive.map(|c| c * material.emissive_strength),
            nearest: material.filter == editor_core::Filter::Nearest,
            unlit: material.unlit,
            blended: tint[3] < 1.0,
        }
    }
}
