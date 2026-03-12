mod camera;
mod chunk;
mod renderer;

use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};

use camera::Camera;
use chunk::{generate_test_chunk, Chunk};
use renderer::Renderer;
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
    chunk: Chunk,
    input: InputState,
    last_frame: Instant,
    cursor_grabbed: bool,
}

impl App {
    fn new() -> Self {
        Self {
            window: None,
            renderer: None,
            camera: Camera::default(),
            chunk: generate_test_chunk(),
            input: InputState::default(),
            last_frame: Instant::now(),
            cursor_grabbed: false,
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
        let renderer = Renderer::new(window.clone())?;
        renderer.upload_chunk(self.chunk.data());
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
                    let uniforms = self.camera.to_uniforms(renderer.aspect());
                    renderer.render(&uniforms);
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
