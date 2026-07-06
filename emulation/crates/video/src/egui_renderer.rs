//! egui GPU backend for wgpu hosts.
//!
//! [`EguiRenderer`] wraps `egui_wgpu::Renderer` and draws already-tessellated
//! egui output as an overlay on top of whatever is already in the surface
//! texture. It mirrors the [`crate::VideoOutput`]/[`crate::RenderTarget`] shape so
//! a wgpu shell drives it the same way it drives [`crate::PixelBufferRenderer`]:
//! the host acquires and presents the surface, and this records+submits its own
//! pass against the supplied `view`. The tessellation itself (which needs an
//! `egui::Context` and a windowing input backend) stays in the host; this crate
//! only owns the GPU side, so it depends on no windowing library.

use egui_wgpu::{Renderer, ScreenDescriptor};

use crate::RenderTarget;

/// The tessellated egui output for one frame, produced by the host from its
/// `egui::Context`. Handed to [`EguiRenderer::render`].
pub struct EguiPaint {
    pub primitives: Vec<egui::ClippedPrimitive>,
    pub textures_delta: egui::TexturesDelta,
    pub pixels_per_point: f32,
}

/// Renders [`EguiPaint`] over the current surface contents.
pub struct EguiRenderer {
    renderer: Renderer,
}

impl EguiRenderer {
    /// Build against the surface color format (no depth, single sample, no
    /// dithering — the emulator overlay needs none of them).
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> EguiRenderer {
        EguiRenderer {
            renderer: Renderer::new(device, format, None, 1, false),
        }
    }

    /// Draw `paint` onto `target.view` with `LoadOp::Load`, preserving whatever
    /// was rendered underneath (the emulator frame). Records and submits its own
    /// command buffer, then frees any textures egui retired this frame.
    pub fn render(&mut self, target: RenderTarget<'_>, paint: &EguiPaint) {
        for (id, delta) in &paint.textures_delta.set {
            self.renderer
                .update_texture(target.device, target.queue, *id, delta);
        }

        let screen = ScreenDescriptor {
            size_in_pixels: [target.surface.width, target.surface.height],
            pixels_per_point: paint.pixels_per_point,
        };

        let mut encoder = target
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("egui-encoder"),
            });

        // `update_buffers` may emit prep command buffers (e.g. texture uploads
        // via user callbacks); they must run before the render pass below.
        let prep = self.renderer.update_buffers(
            target.device,
            target.queue,
            &mut encoder,
            &paint.primitives,
            &screen,
        );

        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target.view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            // egui-wgpu 0.29 requires a `'static` pass (wgpu 22 `forget_lifetime`).
            let mut pass = pass.forget_lifetime();
            self.renderer.render(&mut pass, &paint.primitives, &screen);
        }

        target
            .queue
            .submit(prep.into_iter().chain(std::iter::once(encoder.finish())));

        for id in &paint.textures_delta.free {
            self.renderer.free_texture(id);
        }
    }
}
