use glam::{Mat4, Vec2};
use noise::{Fbm, NoiseFn, Perlin};

use crate::camera::Rect;
use crate::graphics::immediate::{Color, Frame, TextureVertex2D};
use crate::graphics::wgpu_utils::Texture;

use super::{ParamDef, ParamRange, ParamValue, ParameterizedGraphics};

const TEXTURE_SIZE: u32 = 256;

pub struct PerlinNoise;

pub struct PerlinNoiseParams {
    scale: f32,
    octaves: u32,
    persistence: f32,
    lacunarity: f32,
    seed: u32,
}

pub struct PerlinNoiseInstance {
    texture: Texture,
}

impl ParameterizedGraphics for PerlinNoise {
    type Params = PerlinNoiseParams;
    type Instance = PerlinNoiseInstance;

    fn description(&self) -> &str {
        "Perlin noise with fractal Brownian motion"
    }

    fn param_defs(&self) -> Vec<ParamDef> {
        vec![
            ParamDef {
                description: "Scale".into(),
                range: ParamRange::F32 { min: 1.0, max: 100.0 },
                formatter: None,
            },
            ParamDef {
                description: "Octaves".into(),
                range: ParamRange::U32 { min: 1, max: 8 },
                formatter: None,
            },
            ParamDef {
                description: "Persistence".into(),
                range: ParamRange::F32 { min: 0.0, max: 1.0 },
                formatter: None,
            },
            ParamDef {
                description: "Lacunarity".into(),
                range: ParamRange::F32 { min: 1.0, max: 4.0 },
                formatter: None,
            },
            ParamDef {
                description: "Seed".into(),
                range: ParamRange::U32 { min: 0, max: 1000 },
                formatter: None,
            },
        ]
    }

    fn build_params(&self, values: &[ParamValue]) -> PerlinNoiseParams {
        PerlinNoiseParams {
            scale: values[0].as_f32(),
            octaves: values[1].as_u32(),
            persistence: values[2].as_f32(),
            lacunarity: values[3].as_f32(),
            seed: values[4].as_u32(),
        }
    }

    fn init(
        &self,
        params: &PerlinNoiseParams,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> PerlinNoiseInstance {
        let mut fbm = Fbm::<Perlin>::new(params.seed);
        fbm.octaves = params.octaves as usize;
        fbm.persistence = params.persistence as f64;
        fbm.lacunarity = params.lacunarity as f64;

        let size = TEXTURE_SIZE as usize;
        let mut data = vec![0u8; size * size * 4];
        let inv_scale = 1.0 / params.scale as f64;

        for y in 0..size {
            for x in 0..size {
                let nx = x as f64 * inv_scale;
                let ny = y as f64 * inv_scale;
                let val = fbm.get([nx, ny]);
                let brightness = ((val * 0.5 + 0.5).clamp(0.0, 1.0) * 255.0) as u8;
                let idx = (y * size + x) * 4;
                data[idx] = brightness;
                data[idx + 1] = brightness;
                data[idx + 2] = brightness;
                data[idx + 3] = 255;
            }
        }

        let texture = Texture::from_rgba(device, queue, TEXTURE_SIZE, TEXTURE_SIZE, &data, "perlin_noise");
        PerlinNoiseInstance { texture }
    }

    fn render<'a>(&self, instance: &'a PerlinNoiseInstance, cell_rect: Rect, frame: &mut Frame<'a>) {
        let min = cell_rect.min;
        let max = cell_rect.max;
        let mut mat = frame.texture_material(&instance.texture, Color::WHITE, Mat4::IDENTITY);
        mat.quad(
            TextureVertex2D::new(Vec2::new(min.x, min.y), Vec2::new(0.0, 0.0), Color::WHITE),
            TextureVertex2D::new(Vec2::new(max.x, min.y), Vec2::new(1.0, 0.0), Color::WHITE),
            TextureVertex2D::new(Vec2::new(max.x, max.y), Vec2::new(1.0, 1.0), Color::WHITE),
            TextureVertex2D::new(Vec2::new(min.x, max.y), Vec2::new(0.0, 1.0), Color::WHITE),
        );
    }
}
