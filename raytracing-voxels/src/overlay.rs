use bytemuck::{Pod, Zeroable};
use glam::Vec2;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug, PartialEq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const WHITE: Self = Self {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };
    pub const BLACK: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };

    pub fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub fn to_linear(self) -> [f32; 4] {
        [
            self.r as f32 / 255.0,
            self.g as f32 / 255.0,
            self.b as f32 / 255.0,
            self.a as f32 / 255.0,
        ]
    }
}

pub struct Texture {
    width: u32,
    height: u32,
    pixels: Vec<Rgba>,
}

impl Texture {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![
                Rgba {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 0,
                };
                (width * height) as usize
            ],
        }
    }

    pub fn set_pixel(&mut self, x: u32, y: u32, color: Rgba) {
        if x < self.width && y < self.height {
            self.pixels[(y * self.width + x) as usize] = color;
        }
    }

    pub fn get_pixel(&self, x: u32, y: u32) -> Rgba {
        if x < self.width && y < self.height {
            self.pixels[(y * self.width + x) as usize]
        } else {
            Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 0,
            }
        }
    }

    pub fn data(&self) -> &[Rgba] {
        &self.pixels
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug)]
pub struct OverlayVertex {
    pub pos: [f32; 2],
    pub color: [f32; 4],
    pub uv: [f32; 2],
}

pub fn vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTRS: &[wgpu::VertexAttribute] = &[
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x2,
            offset: 0,
            shader_location: 0,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x4,
            offset: 8,
            shader_location: 1,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x2,
            offset: 24,
            shader_location: 2,
        },
    ];

    wgpu::VertexBufferLayout {
        array_stride: 32,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: ATTRS,
    }
}

pub struct DrawList {
    vertices: Vec<OverlayVertex>,
    indices: Vec<u32>,
}

impl DrawList {
    pub fn new() -> Self {
        Self {
            vertices: Vec::new(),
            indices: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.vertices.clear();
        self.indices.clear();
    }

    pub fn vertex_data(&self) -> &[u8] {
        bytemuck::cast_slice(&self.vertices)
    }

    pub fn index_data(&self) -> &[u8] {
        bytemuck::cast_slice(&self.indices)
    }

    pub fn index_count(&self) -> u32 {
        self.indices.len() as u32
    }

    #[allow(clippy::too_many_arguments)]
    pub fn rect(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        uv_min: Vec2,
        uv_max: Vec2,
        color: Rgba,
    ) {
        let c = color.to_linear();
        let uv_min = uv_min.to_array();
        let uv_max = uv_max.to_array();

        let base = self.vertices.len() as u32;

        self.vertices.push(OverlayVertex {
            pos: [x, y],
            color: c,
            uv: [uv_min[0], uv_min[1]],
        });
        self.vertices.push(OverlayVertex {
            pos: [x + w, y],
            color: c,
            uv: [uv_max[0], uv_min[1]],
        });
        self.vertices.push(OverlayVertex {
            pos: [x + w, y + h],
            color: c,
            uv: [uv_max[0], uv_max[1]],
        });
        self.vertices.push(OverlayVertex {
            pos: [x, y + h],
            color: c,
            uv: [uv_min[0], uv_max[1]],
        });

        self.indices.push(base);
        self.indices.push(base + 1);
        self.indices.push(base + 2);
        self.indices.push(base);
        self.indices.push(base + 2);
        self.indices.push(base + 3);
    }

    pub fn solid_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Rgba) {
        self.rect(x, y, w, h, Vec2::ZERO, Vec2::ONE, color);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn atlas_rect(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        uv_min: Vec2,
        uv_max: Vec2,
        color: Rgba,
    ) {
        self.rect(x, y, w, h, uv_min, uv_max, color);
    }

    pub fn atlas_sprite(
        &mut self,
        atlas: &crate::texture_atlas::TextureAtlas,
        name: &str,
        x: f32,
        y: f32,
        scale: f32,
        tint: Rgba,
    ) -> bool {
        let Some(region) = atlas.region(name) else {
            return false;
        };
        let Some((uv_min, uv_max)) = atlas.uv_rect(name) else {
            return false;
        };
        let w = region.width as f32 * scale;
        let h = region.height as f32 * scale;
        self.rect(x, y, w, h, uv_min, uv_max, tint);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba_rgb_has_full_alpha() {
        let c = Rgba::rgb(255, 0, 0);
        assert_eq!(c.a, 255);
        assert_eq!(c.r, 255);
        assert_eq!(c.g, 0);
        assert_eq!(c.b, 0);
    }

    #[test]
    fn texture_new_is_all_zero() {
        let tex = Texture::new(4, 4);
        let transparent = Rgba {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        };
        for pixel in tex.data() {
            assert_eq!(*pixel, transparent);
        }
    }

    #[test]
    fn texture_set_get_round_trip() {
        let mut tex = Texture::new(8, 8);
        let red = Rgba::rgb(255, 0, 0);
        tex.set_pixel(3, 5, red);
        assert_eq!(tex.get_pixel(3, 5), red);
    }

    #[test]
    fn texture_oob_get_returns_transparent() {
        let tex = Texture::new(4, 4);
        let transparent = Rgba {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        };
        assert_eq!(tex.get_pixel(10, 10), transparent);
    }

    #[test]
    fn texture_oob_set_is_noop() {
        let mut tex = Texture::new(4, 4);
        let transparent = Rgba {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        };
        tex.set_pixel(10, 10, Rgba::WHITE);
        for pixel in tex.data() {
            assert_eq!(*pixel, transparent);
        }
    }

    #[test]
    fn drawlist_new_is_empty() {
        let dl = DrawList::new();
        assert_eq!(dl.vertices.len(), 0);
        assert_eq!(dl.indices.len(), 0);
    }

    #[test]
    fn drawlist_clear_empties() {
        let mut dl = DrawList::new();
        dl.solid_rect(0.0, 0.0, 10.0, 10.0, Rgba::WHITE);
        assert!(dl.vertices.len() > 0);
        dl.clear();
        assert_eq!(dl.vertices.len(), 0);
        assert_eq!(dl.indices.len(), 0);
    }

    #[test]
    fn rect_appends_4_verts_6_indices() {
        let mut dl = DrawList::new();
        dl.rect(
            0.0,
            0.0,
            10.0,
            20.0,
            Vec2::ZERO,
            Vec2::ONE,
            Rgba::WHITE,
        );
        assert_eq!(dl.vertices.len(), 4);
        assert_eq!(dl.indices.len(), 6);
    }

    #[test]
    fn two_rects_produce_correct_counts_and_offsets() {
        let mut dl = DrawList::new();
        dl.solid_rect(0.0, 0.0, 10.0, 10.0, Rgba::WHITE);
        dl.solid_rect(20.0, 20.0, 5.0, 5.0, Rgba::BLACK);
        assert_eq!(dl.vertices.len(), 8);
        assert_eq!(dl.indices.len(), 12);
        assert_eq!(dl.indices[6], 4);
        assert_eq!(dl.indices[7], 5);
        assert_eq!(dl.indices[8], 6);
        assert_eq!(dl.indices[9], 4);
        assert_eq!(dl.indices[10], 6);
        assert_eq!(dl.indices[11], 7);
    }

    #[test]
    fn atlas_sprite_valid_name_produces_geometry() {
        let mut builder = crate::texture_atlas::TextureAtlasBuilder::new();
        builder.add("sprite", Texture::new(16, 16));
        let atlas = builder.build().unwrap();
        let mut dl = DrawList::new();
        let found = dl.atlas_sprite(&atlas, "sprite", 0.0, 0.0, 1.0, Rgba::WHITE);
        assert!(found);
        assert_eq!(dl.vertices.len(), 4);
        assert_eq!(dl.indices.len(), 6);
    }

    #[test]
    fn atlas_sprite_invalid_name_returns_false() {
        let builder = crate::texture_atlas::TextureAtlasBuilder::new();
        let atlas = builder.build().unwrap();
        let mut dl = DrawList::new();
        let found = dl.atlas_sprite(&atlas, "nope", 0.0, 0.0, 1.0, Rgba::WHITE);
        assert!(!found);
        assert_eq!(dl.vertices.len(), 0);
    }

    #[test]
    fn vertex_positions_match_input() {
        let mut dl = DrawList::new();
        dl.rect(
            5.0,
            10.0,
            20.0,
            30.0,
            Vec2::ZERO,
            Vec2::ONE,
            Rgba::WHITE,
        );
        assert_eq!(dl.vertices[0].pos, [5.0, 10.0]);
        assert_eq!(dl.vertices[1].pos, [25.0, 10.0]);
        assert_eq!(dl.vertices[2].pos, [25.0, 40.0]);
        assert_eq!(dl.vertices[3].pos, [5.0, 40.0]);
    }
}
