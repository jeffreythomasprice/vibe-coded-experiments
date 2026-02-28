use std::sync::Arc;

use anyhow::Result;
use glam::{Mat3, Vec2, Vec4};

use crate::gfx::{Mesh, Texture, Vertex2d};
use crate::renderer::{Actor, Renderer};
use crate::resources::Text;
use crate::state_machine::GfxState;

pub struct Menu {
    renderer: Renderer,
    _texts: Vec<Text>,
}

impl Menu {
    pub fn new(gfx: &GfxState, lines: &[&str], scale: f32) -> Result<Self> {
        use ab_glyph::{Font, PxScale, ScaleFont};

        let font = ab_glyph::FontArc::try_from_slice(include_bytes!(
            "../resources/IntelOneMono-Regular.ttf"
        ))?;
        let px_scale = PxScale::from(scale);
        let scaled = font.as_scaled(px_scale);
        let ascent = scaled.ascent();
        let line_height = ascent - scaled.descent() + scaled.line_gap();

        let n = lines.len() as f32;
        let y_start = 240.0 - n * line_height / 2.0 + ascent;

        let mut renderer = Renderer::new(gfx)?;

        let white_pixel = [255u8, 255, 255, 255];
        let overlay_tex = Arc::new(Texture::new_with_image_bytes(
            &gfx.device, &gfx.queue, 1, 1, &white_pixel,
        ));
        let mut overlay_mesh = Mesh::<Vertex2d>::new(&gfx.device, 4, 6);
        let color = Vec4::new(0.0, 0.0, 0.0, 0.6);
        overlay_mesh.append_quad(
            &gfx.queue,
            Vertex2d { position: Vec2::new(0.0,   0.0),   uv: Vec2::ZERO, color },
            Vertex2d { position: Vec2::new(640.0, 0.0),   uv: Vec2::ZERO, color },
            Vertex2d { position: Vec2::new(640.0, 480.0), uv: Vec2::ZERO, color },
            Vertex2d { position: Vec2::new(0.0,   480.0), uv: Vec2::ZERO, color },
        );
        renderer.add_actor(Actor {
            mesh: overlay_mesh,
            texture: overlay_tex,
            transform: Mat3::IDENTITY,
            z_index: -1,
        });

        let mut texts = Vec::new();

        for (i, &line) in lines.iter().enumerate() {
            if line.is_empty() {
                continue;
            }
            let mut text = Text::new(&gfx.device, font.clone(), scale);
            let width = text.measure_width(line);
            let x = (640.0 - width) / 2.0;
            let y = y_start + i as f32 * line_height;

            let mut mesh = Mesh::new(&gfx.device, line.len() * 4, line.len() * 6);
            text.render_text(&gfx.device, &gfx.queue, &mut mesh, line);
            let texture = text.texture.clone();
            renderer.add_actor(Actor {
                mesh,
                texture,
                transform: Mat3::from_translation(glam::Vec2::new(x, y)),
                z_index: 0,
            });
            texts.push(text);
        }

        Ok(Self {
            renderer,
            _texts: texts,
        })
    }

    pub fn draw(
        &mut self,
        gfx: &GfxState,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        clear_color: Option<wgpu::Color>,
    ) -> Result<()> {
        self.renderer.draw(gfx, encoder, view, clear_color)
    }
}
