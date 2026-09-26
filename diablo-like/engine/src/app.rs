use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;

use web_time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::PhysicalKey;
use winit::window::{Window, WindowAttributes, WindowId};

use glam::Vec2;

use crate::error::Error;
use crate::geom::contour;
use crate::geom::parts::Level;
use crate::geom::tess;
use crate::input::InputState;
use crate::physics::{Actor, World};
use crate::render::camera_ubo::CameraUbo;
use crate::render::chunks::{ChunkAtlas, ChunkPipeline, ChunkStats};
use crate::render::composite::CompositePipeline;
use crate::render::crisp::{self, CrispPipeline, GpuMesh};
use crate::render::gpu::Gpu;
use crate::render::offscreen::Offscreen;
use crate::render::streaming_fixture;
use crate::render::text_pipeline::TextPipeline;
use crate::render::RenderMode;
use crate::sim::Camera;
use crate::text::{Atlas, Font};

const RENDER_SCALES: [f32; 3] = [1.0, 1.5, 2.0];
/// How far ahead of the camera's current velocity to bias chunk loading, so
/// chunks are ready before they're visible rather than after (see the
/// design plan's Chunked field section).
const CHUNK_LOOKAHEAD_SECONDS: f32 = 0.5;
const CHUNK_BAKE_BUDGET: std::time::Duration = std::time::Duration::from_micros(1500);

/// Lets `engine` own the async wgpu init and the winit loop while staying
/// agnostic to how each target drives a future to completion: desktop passes
/// `pollster::block_on`, web passes `wasm_bindgen_futures::spawn_local`. See
/// the "Async seam" note in the design plan for why this replaces a separate
/// platform module.
pub type SpawnFn = fn(Pin<Box<dyn Future<Output = ()> + 'static>>);

pub struct EngineConfig {
    pub title: &'static str,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            title: "diablo-like",
        }
    }
}

pub fn run(spawn: SpawnFn, config: EngineConfig) -> Result<(), Error> {
    let event_loop = EventLoop::new()?;
    let app = App::new(spawn, config);

    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut app = app;
        event_loop.run_app(&mut app)?;
    }

    #[cfg(target_arch = "wasm32")]
    {
        use winit::platform::web::EventLoopExtWebSys;
        event_loop.spawn_app(app);
    }

    Ok(())
}

/// Bundles the device/surface handle together with the GPU resources that
/// depend on it. `Gpu` itself stays limited to device/surface/capability
/// concerns (see render/gpu.rs); this is where per-scene rendering state
/// lives until there's enough of it to warrant its own module.
struct Renderer {
    gpu: Gpu,
    camera_ubo: CameraUbo,
    atlas: Atlas,
    text_pipeline: TextPipeline,
    level: Level,
    chunk_atlas: ChunkAtlas,
    chunk_pipeline: ChunkPipeline,
    crisp_pipeline: CrispPipeline,
    fill_mesh: GpuMesh,
    stroke_mesh: GpuMesh,
    composite_pipeline: CompositePipeline,
    offscreen: Offscreen,
    offscreen_format: wgpu::TextureFormat,
}

impl Renderer {
    async fn new(window: Arc<Window>, render_scale: f32) -> Result<Self, Error> {
        let gpu = Gpu::new(window).await?;

        // The world pass (chunked glow field + crisp fill/stroke + physics
        // debug markers) renders into this at `render_scale`x resolution;
        // falls back to Rgba8Unorm wherever the HDR-render-attachment probe
        // in gpu.rs came back empty.
        let offscreen_format = gpu
            .caps
            .hdr_format
            .unwrap_or(wgpu::TextureFormat::Rgba8Unorm);

        let camera_ubo = CameraUbo::new(&gpu.device, glam::Mat4::IDENTITY);

        let font = Font::load()?;
        let atlas = font.build_atlas(&gpu.device, &gpu.queue, HUD_FONT_PX);
        let text_pipeline =
            TextPipeline::new(&gpu.device, gpu.config.format, &atlas.bind_group_layout);

        // M6's streaming fixture (see render/streaming_fixture.rs): a chain
        // of rooms and corridors spanning many chunks, so the chunk
        // residency system in render/chunks.rs has something worth
        // streaming across as the camera flies over it.
        let (level, contour_root) = streaming_fixture::build();

        let chunk_atlas = ChunkAtlas::new(&gpu.device, &gpu.queue);
        let chunk_pipeline = ChunkPipeline::new(
            &gpu.device,
            offscreen_format,
            &camera_ubo,
            &chunk_atlas.bind_group_layout,
        );

        // The exact boolean-composed geometry, tessellated once at startup
        // — contour/tess cost scales with primitive count, not world size,
        // so unlike the field this doesn't need chunking (see the design
        // plan's note in streaming_fixture.rs).
        let crisp_pipeline = CrispPipeline::new(&gpu.device, offscreen_format, &camera_ubo);
        let contour_shapes = contour::extract(&level.csg, contour_root, 0.01)
            .expect("streaming fixture overlay should succeed");
        let fill_mesh = GpuMesh::new(
            &gpu.device,
            &tess::fill(&contour_shapes, 0.01)
                .expect("streaming fixture fill tessellation should succeed"),
        );
        let stroke_mesh = GpuMesh::new(
            &gpu.device,
            &tess::stroke(&contour_shapes, STROKE_WIDTH, 0.01)
                .expect("streaming fixture stroke tessellation should succeed"),
        );

        let composite_pipeline = CompositePipeline::new(&gpu.device, gpu.config.format);
        let (ow, oh) = Offscreen::scaled_size(
            gpu.config.width,
            gpu.config.height,
            render_scale,
            gpu.caps.max_texture_dimension_2d,
        );
        let offscreen = Offscreen::new(
            &gpu.device,
            &composite_pipeline.bind_group_layout,
            offscreen_format,
            ow,
            oh,
        );

        Ok(Self {
            gpu,
            camera_ubo,
            atlas,
            text_pipeline,
            level,
            chunk_atlas,
            chunk_pipeline,
            crisp_pipeline,
            fill_mesh,
            stroke_mesh,
            composite_pipeline,
            offscreen,
            offscreen_format,
        })
    }

    fn resize_offscreen(&mut self, render_scale: f32) {
        let (w, h) = Offscreen::scaled_size(
            self.gpu.config.width,
            self.gpu.config.height,
            render_scale,
            self.gpu.caps.max_texture_dimension_2d,
        );
        if w == self.offscreen.width && h == self.offscreen.height {
            return;
        }
        self.offscreen = Offscreen::new(
            &self.gpu.device,
            &self.composite_pipeline.bind_group_layout,
            self.offscreen_format,
            w,
            h,
        );
    }
}

const FIXED_DT: f64 = 1.0 / 120.0;
// Close enough to see wall contact/sliding behaviour clearly, per M7's
// verify step — M6's wider free-fly view isn't needed now that the camera
// follows a collision-constrained actor instead of panning freely.
const CAMERA_HALF_EXTENT_Y: f32 = 6.0;
const HUD_FONT_PX: f32 = 22.0;
const HUD_MARGIN: f32 = 10.0;
const HUD_LINE_HEIGHT: f32 = 26.0;
const STROKE_WIDTH: f32 = 0.08;
const ACTOR_RADIUS: f32 = 0.4;
const ACTOR_SPEED: f32 = 5.0;
const ACTOR_MARKER_SEGMENTS: u32 = 16;
const CONTACT_NORMAL_LENGTH: f32 = 0.6;
const CONTACT_NORMAL_WIDTH: f32 = 0.04;

struct App {
    spawn: SpawnFn,
    config: EngineConfig,
    window: Option<Arc<Window>>,
    renderer: Rc<RefCell<Option<Renderer>>>,
    input: InputState,
    camera: Camera,
    start: Instant,
    last_update: Instant,
    accumulator: f64,
    tick_count: u64,
    last_log: Instant,
    frame_count: u64,
    last_log_frame_count: u64,
    hud_fps: f32,
    render_scale: f32,
    render_mode: RenderMode,
    world: World,
    chunk_stats: ChunkStats,
}

impl App {
    fn new(spawn: SpawnFn, config: EngineConfig) -> Self {
        let now = Instant::now();
        Self {
            spawn,
            config,
            window: None,
            renderer: Rc::new(RefCell::new(None)),
            input: InputState::default(),
            camera: Camera::new(CAMERA_HALF_EXTENT_Y),
            start: now,
            last_update: now,
            accumulator: 0.0,
            tick_count: 0,
            last_log: now,
            frame_count: 0,
            last_log_frame_count: 0,
            hud_fps: 0.0,
            render_scale: RENDER_SCALES[1],
            render_mode: RenderMode::Both,
            world: World::new(Actor::new(Vec2::ZERO, ACTOR_RADIUS)),
            chunk_stats: ChunkStats::default(),
        }
    }

    fn cycle_render_scale(&mut self) {
        let current = RENDER_SCALES
            .iter()
            .position(|&s| s == self.render_scale)
            .unwrap_or(0);
        self.render_scale = RENDER_SCALES[(current + 1) % RENDER_SCALES.len()];
        if let Some(renderer) = self.renderer.borrow_mut().as_mut() {
            renderer.resize_offscreen(self.render_scale);
        }
    }

    fn update(&mut self) {
        let now = Instant::now();
        // Clamped so a backgrounded tab/window doesn't run hundreds of ticks
        // in one go when it regains focus.
        let frame_dt = (now - self.last_update).as_secs_f64().min(0.1);
        self.last_update = now;
        self.accumulator += frame_dt;

        while self.accumulator >= FIXED_DT {
            self.step(FIXED_DT as f32);
            self.accumulator -= FIXED_DT;
            self.tick_count += 1;
        }

        let since_log = now.duration_since(self.last_log).as_secs_f64();
        if since_log >= 2.0 {
            let wall_seconds = now.duration_since(self.start).as_secs_f64();
            let expected_ticks = (wall_seconds / FIXED_DT) as u64;
            self.hud_fps = ((self.frame_count - self.last_log_frame_count) as f64 / since_log) as f32;
            tracing::trace!(
                ticks = self.tick_count,
                expected_ticks,
                wall_seconds,
                fps = self.hud_fps,
                "tick accumulator"
            );
            self.last_log = now;
            self.last_log_frame_count = self.frame_count;
        }
    }

    fn step(&mut self, dt: f32) {
        self.world.actor.vel = self.input.movement() * ACTOR_SPEED;
        // Physics needs the level (owned by the renderer, since it's also
        // what the chunk atlas bakes against), which isn't ready until the
        // async GPU/renderer init completes — skip stepping until then
        // rather than moving the actor through geometry it can't collide
        // with yet.
        if let Some(renderer) = self.renderer.borrow().as_ref() {
            self.world.step(&renderer.level, dt);
        }
        self.camera.position = self.world.actor.pos;
    }

    fn render(&mut self) {
        let mut renderer_ref = self.renderer.borrow_mut();
        let Some(renderer) = renderer_ref.as_mut() else {
            return;
        };
        if renderer.gpu.config.width == 0 || renderer.gpu.config.height == 0 {
            return;
        }

        self.frame_count += 1;

        renderer
            .camera_ubo
            .write(&renderer.gpu.queue, self.camera.view_proj());

        let focus = self.world.actor.pos + self.world.actor.vel * CHUNK_LOOKAHEAD_SECONDS;
        self.chunk_stats = renderer.chunk_atlas.update(
            &renderer.gpu.device,
            &renderer.gpu.queue,
            &renderer.level,
            focus,
            CHUNK_BAKE_BUDGET,
        );

        let surface_texture = match renderer.gpu.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return;
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                renderer.gpu.surface.configure(&renderer.gpu.device, &renderer.gpu.config);
                return;
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                tracing::error!("surface validation error");
                return;
            }
        };

        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder =
            renderer
                .gpu
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("frame"),
                });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("world"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: renderer.offscreen.view(),
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // Matches what a void chunk's own glow shader
                        // renders as (see chunk_glow.wgsl): near-enough to
                        // exactly black that there's no visible seam at the
                        // boundary between "a void chunk quad was drawn
                        // here" and "no chunk quad was drawn here at all".
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if matches!(self.render_mode, RenderMode::Glow | RenderMode::Both) {
                renderer.chunk_pipeline.draw(
                    &renderer.gpu.device,
                    &mut pass,
                    &renderer.camera_ubo,
                    &renderer.chunk_atlas,
                );
            }
            if matches!(self.render_mode, RenderMode::Crisp | RenderMode::Both) {
                renderer
                    .crisp_pipeline
                    .draw_fill(&mut pass, &renderer.camera_ubo, &renderer.fill_mesh);
            }
            // The exact contour stroke is drawn in every mode: it's the
            // cross-check between the baked field and the exact geometry,
            // not something exclusive to crisp mode.
            renderer
                .crisp_pipeline
                .draw_stroke(&mut pass, &renderer.camera_ubo, &renderer.stroke_mesh);

            // Physics debug visualization: the actor itself, plus a short
            // normal indicator at each contact the last tick made.
            let marker = GpuMesh::new(
                &renderer.gpu.device,
                &crisp::circle_marker_mesh(self.world.actor.pos, ACTOR_RADIUS, ACTOR_MARKER_SEGMENTS),
            );
            renderer
                .crisp_pipeline
                .draw_stroke(&mut pass, &renderer.camera_ubo, &marker);
            for contact in &self.world.contacts {
                let normal_mesh = crisp::line_segment_mesh(
                    contact.point,
                    contact.point + contact.normal * CONTACT_NORMAL_LENGTH,
                    CONTACT_NORMAL_WIDTH,
                );
                let gpu_mesh = GpuMesh::new(&renderer.gpu.device, &normal_mesh);
                renderer
                    .crisp_pipeline
                    .draw_stroke(&mut pass, &renderer.camera_ubo, &gpu_mesh);
            }
        }

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("composite + overlay"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            renderer
                .composite_pipeline
                .draw(&mut pass, renderer.offscreen.bind_group());

            let screen_w = renderer.gpu.config.width as f32;
            let screen_h = renderer.gpu.config.height as f32;
            let hud = [
                format!("FPS  {:.0}", self.hud_fps),
                format!("TICK {}", self.tick_count),
                format!("CAM  {:.2}, {:.2}", self.camera.position.x, self.camera.position.y),
                format!("SCALE {:.2}", self.render_scale),
                format!("MODE {}", self.render_mode.label()),
                format!("RES  {}", self.chunk_stats.resident),
                format!("QUE  {}", self.chunk_stats.queued),
                format!("BAKE {:.2}ms", self.chunk_stats.last_bake_ms),
                format!("EVICT {}", self.chunk_stats.evicted_total),
                format!("TRACE {}", self.world.max_trace_iterations),
            ];
            for (i, line) in hud.iter().enumerate() {
                let vertices = crate::text::layout(
                    &renderer.atlas,
                    screen_w,
                    screen_h,
                    HUD_MARGIN,
                    HUD_MARGIN + i as f32 * HUD_LINE_HEIGHT,
                    line,
                );
                renderer
                    .text_pipeline
                    .draw(&renderer.gpu.device, &mut pass, &renderer.atlas, &vertices);
            }
        }

        renderer.gpu.queue.submit(Some(encoder.finish()));
        surface_texture.present();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attrs = WindowAttributes::default().with_title(self.config.title);

        #[cfg(target_arch = "wasm32")]
        let attrs = {
            use wasm_bindgen::JsCast;
            use winit::platform::web::WindowAttributesExtWebSys;
            let canvas = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.get_element_by_id("canvas"))
                .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok());
            attrs.with_canvas(canvas)
        };

        let window = match event_loop.create_window(attrs) {
            Ok(window) => Arc::new(window),
            Err(err) => {
                tracing::error!("failed to create window: {err}");
                event_loop.exit();
                return;
            }
        };

        let size = window.inner_size();
        self.camera
            .set_viewport(size.width as f32, size.height as f32);

        self.window = Some(window.clone());

        let renderer_slot = self.renderer.clone();
        let render_scale = self.render_scale;
        let fut = async move {
            match Renderer::new(window, render_scale).await {
                Ok(renderer) => *renderer_slot.borrow_mut() = Some(renderer),
                Err(err) => tracing::error!("renderer init failed: {err}"),
            }
        };
        (self.spawn)(Box::pin(fut));
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                self.camera
                    .set_viewport(size.width as f32, size.height as f32);
                if let Some(renderer) = self.renderer.borrow_mut().as_mut() {
                    renderer.gpu.resize(size.width, size.height);
                    renderer.resize_offscreen(self.render_scale);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    self.input.set_key(code, event.state == ElementState::Pressed);
                    if event.state == ElementState::Pressed && !event.repeat {
                        match code {
                            winit::keyboard::KeyCode::KeyR => self.cycle_render_scale(),
                            winit::keyboard::KeyCode::KeyM => {
                                self.render_mode = self.render_mode.next();
                            }
                            _ => {}
                        }
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                self.render();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        self.update();
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}
