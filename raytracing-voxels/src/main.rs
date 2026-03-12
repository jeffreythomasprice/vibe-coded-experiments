mod camera;
mod chunk;
mod overlay;
mod overlay_renderer;
mod texture_atlas;
mod texture_atlas_font;
mod voxel_renderer;
mod voxel_textures;
mod world;

use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};

use camera::Camera;
use chunk::generate_test_chunk;
use overlay::{DrawList, Rgba};
use texture_atlas_font::TextureAtlasFont;
use voxel_renderer::Renderer;
use world::World;

const MINECRAFT_FONT: &[u8] = include_bytes!("../resources/Minecraft.otf");
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, KeyEvent, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

const MOVE_SPEED: f32 = 7.5;
const MOUSE_SENSITIVITY: f32 = 0.003;
const MAX_DT: f32 = 0.1;

#[derive(Default)]
struct InputState {
    forward: bool,
    back: bool,
    left: bool,
    right: bool,
    up: bool,
    down: bool,
}

struct App {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    camera: Camera,
    world: World,
    input: InputState,
    last_frame: Instant,
    cursor_grabbed: bool,
    font: Option<TextureAtlasFont>,
    font_bind_group: Option<wgpu::BindGroup>,
    draw_list: DrawList,
    frame_count: u32,
    fps_accum: f32,
    fps_display: f32,
}

impl App {
    fn new() -> Self {
        let mut world = World::new();
        for x in -1..=1 {
            for z in -1..=1 {
                world.insert([x, 0, z], generate_test_chunk());
            }
        }

        Self {
            window: None,
            renderer: None,
            camera: Camera::new([24.0, 20.0, 40.0], {
                let dx: f32 = 0.0 - 24.0;
                let dz: f32 = 0.0 - 40.0;
                f32::atan2(-dx, -dz)
            }, {
                let dx: f32 = 0.0 - 24.0;
                let dy: f32 = 0.0 - 20.0;
                let dz: f32 = 0.0 - 40.0;
                let h = (dx * dx + dz * dz).sqrt();
                f32::atan2(dy, h)
            }, 60.0_f32.to_radians()),
            world,
            input: InputState::default(),
            last_frame: Instant::now(),
            cursor_grabbed: false,
            font: None,
            font_bind_group: None,
            draw_list: DrawList::new(),
            frame_count: 0,
            fps_accum: 0.0,
            fps_display: 0.0,
        }
    }

    fn grab_cursor(&mut self) {
        if let Some(window) = &self.window
            && window
                .set_cursor_grab(CursorGrabMode::Locked)
                .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined))
                .is_ok()
        {
            window.set_cursor_visible(false);
            self.cursor_grabbed = true;
        }
    }

    fn release_cursor(&mut self) {
        if let Some(window) = &self.window {
            let _ = window.set_cursor_grab(CursorGrabMode::None);
            window.set_cursor_visible(true);
            self.cursor_grabbed = false;
        }
    }

}

impl App {
    fn try_resume(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        let window_attrs = Window::default_attributes().with_title("Raytracing Voxels");
        let window = Arc::new(
            event_loop
                .create_window(window_attrs)
                .context("failed to create window")?,
        );
        let mut renderer = Renderer::new(window.clone())?;
        let (voxel_data, chunk_infos) = self.world.pack_gpu_data();
        renderer.upload_world(&voxel_data, &chunk_infos, chunk_infos.len() as u32);
        self.world.clear_dirty();

        let atlas = voxel_textures::build_voxel_atlas(42)?;
        renderer.upload_voxel_atlas(atlas.texture(), atlas.uv_map());

        let ascii_charset: String = (0x20u8..=0x7E).map(|b| b as char).collect();
        let font = TextureAtlasFont::new(MINECRAFT_FONT, 32.0, &ascii_charset)
            .context("failed to create font")?;
        self.font_bind_group = Some(
            renderer
                .overlay()
                .create_texture(renderer.device(), renderer.queue(), font.atlas().texture()),
        );
        self.font = Some(font);

        self.window = Some(window);
        self.renderer = Some(renderer);
        self.last_frame = Instant::now();
        Ok(())
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        if let Err(e) = self.try_resume(event_loop) {
            log::error!("failed to initialize: {e:#}");
            event_loop.exit();
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        if !self.cursor_grabbed {
            return;
        }
        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            self.camera
                .rotate(-dx as f32 * MOUSE_SENSITIVITY, -dy as f32 * MOUSE_SENSITIVITY);
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state: ElementState::Pressed,
                ..
            } => {
                if !self.cursor_grabbed {
                    self.grab_cursor();
                }
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        logical_key: Key::Named(NamedKey::Escape),
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            } => {
                if self.cursor_grabbed {
                    self.release_cursor();
                } else {
                    event_loop.exit();
                }
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(code),
                        state,
                        ..
                    },
                ..
            } => {
                let pressed = state == ElementState::Pressed;
                match code {
                    KeyCode::KeyW => self.input.forward = pressed,
                    KeyCode::KeyS => self.input.back = pressed,
                    KeyCode::KeyA => self.input.left = pressed,
                    KeyCode::KeyD => self.input.right = pressed,
                    KeyCode::Space => self.input.up = pressed,
                    KeyCode::ShiftLeft => self.input.down = pressed,
                    _ => {}
                }
            }
            WindowEvent::Resized(new_size) => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(new_size.width, new_size.height);
                }
            }
            WindowEvent::RedrawRequested => {
                let dt = self.last_frame.elapsed().as_secs_f32().min(MAX_DT);
                self.last_frame = Instant::now();

                self.frame_count += 1;
                self.fps_accum += dt;
                if self.fps_accum >= 1.0 {
                    self.fps_display = self.frame_count as f32 / self.fps_accum;
                    self.frame_count = 0;
                    self.fps_accum = 0.0;
                }

                let move_dist = MOVE_SPEED * dt;
                if self.input.forward {
                    self.camera.move_forward(move_dist);
                }
                if self.input.back {
                    self.camera.move_forward(-move_dist);
                }
                if self.input.right {
                    self.camera.move_right(move_dist);
                }
                if self.input.left {
                    self.camera.move_right(-move_dist);
                }
                if self.input.up {
                    self.camera.move_up(move_dist);
                }
                if self.input.down {
                    self.camera.move_up(-move_dist);
                }

                if let (Some(renderer), Some(window)) = (&mut self.renderer, &self.window) {
                    if self.world.is_dirty() {
                        let (voxel_data, chunk_infos) = self.world.pack_gpu_data();
                        renderer.upload_world(&voxel_data, &chunk_infos, chunk_infos.len() as u32);
                        self.world.clear_dirty();
                    }

                    let uniforms = self.camera.to_uniforms(renderer.aspect());

                    if let Some((frame, view)) = renderer.begin_frame() {
                        let mut encoder = renderer.create_encoder();
                        renderer.render_voxels(&mut encoder, &view, &uniforms);

                        self.draw_list.clear();
                        if let Some(font) = &self.font {
                            let fps_text = format!("FPS: {:.0}", self.fps_display);
                            font.draw_text(
                                &mut self.draw_list,
                                &fps_text,
                                10.0,
                                10.0,
                                Rgba::WHITE,
                            );
                        }

                        if let Some(texture_bg) = &self.font_bind_group {
                            renderer.render_overlay(
                                &view,
                                &mut encoder,
                                &self.draw_list,
                                texture_bg,
                            );
                        }

                        renderer.submit(encoder);
                        frame.present();
                    }
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

fn main() -> Result<()> {
    env_logger::init();
    let event_loop = EventLoop::new().context("failed to create event loop")?;
    let mut app = App::new();
    event_loop
        .run_app(&mut app)
        .context("event loop error")?;
    Ok(())
}
