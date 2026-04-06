use std::sync::Arc;
use std::time::Instant;

use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{Key, NamedKey},
    window::{Window, WindowAttributes, WindowId},
};

use crate::egui_integration::EguiIntegration;
use crate::graphics::immediate::ImmediateRenderer;
use crate::graphics::wgpu_utils::GpuState;
use crate::menu::OverlayState;
use crate::state::{AppState, EventContext, InitContext, RenderContext, StateTransition, UpdateContext};

struct App {
    window: Option<Arc<Window>>,
    gpu: Option<GpuState>,
    renderer: Option<ImmediateRenderer>,
    state: Option<Box<dyn AppState>>,
    state_factory: Option<Box<dyn FnOnce(&InitContext) -> Box<dyn AppState>>>,
    last_frame_time: Option<Instant>,
    egui: Option<EguiIntegration>,
    overlay_state: OverlayState,
}

impl App {
    fn new(factory: Box<dyn FnOnce(&InitContext) -> Box<dyn AppState>>) -> Self {
        Self {
            window: None,
            gpu: None,
            renderer: None,
            state: None,
            state_factory: Some(factory),
            last_frame_time: None,
            egui: None,
            overlay_state: OverlayState::Hidden,
        }
    }

    fn apply_transition(&mut self, transition: StateTransition, event_loop: &ActiveEventLoop) {
        match transition {
            StateTransition::Continue => {}
            StateTransition::Switch(new_state) => {
                self.state = Some(new_state);
            }
            StateTransition::Quit => {
                event_loop.exit();
            }
        }
    }
}

fn is_escape_press(event: &WindowEvent) -> bool {
    matches!(
        event,
        WindowEvent::KeyboardInput {
            event: winit::event::KeyEvent {
                logical_key: Key::Named(NamedKey::Escape),
                state: winit::event::ElementState::Pressed,
                ..
            },
            ..
        }
    )
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            tracing::debug!("Resumed (window already exists)");
            return;
        }

        tracing::info!("Creating window");
        let attrs = WindowAttributes::default().with_title("Procedural Art Generator");
        let window = Arc::new(
            event_loop
                .create_window(attrs)
                .expect("failed to create window"),
        );

        tracing::info!("Initializing GPU state");
        let gpu = GpuState::new(window.clone());
        let renderer = ImmediateRenderer::new(&gpu.device, &gpu.queue, gpu.config.format);
        let egui = EguiIntegration::new(&window, &gpu.device, gpu.config.format);

        if let Some(factory) = self.state_factory.take() {
            let init_ctx = InitContext {
                device: &gpu.device,
                queue: &gpu.queue,
                surface_format: gpu.config.format,
                viewport_size: (gpu.config.width, gpu.config.height),
            };
            self.state = Some(factory(&init_ctx));
        }

        self.window = Some(window);
        self.renderer = Some(renderer);
        self.gpu = Some(gpu);
        self.egui = Some(egui);
        tracing::info!("Application ready");
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                tracing::info!("Close requested, exiting");
                event_loop.exit();
            }
            WindowEvent::Resized(new_size) => {
                if let Some(gpu) = &mut self.gpu {
                    gpu.resize(new_size);
                }
                if let Some(state) = &mut self.state {
                    let gpu = self.gpu.as_ref().unwrap();
                    let ctx = EventContext {
                        viewport_size: (gpu.config.width, gpu.config.height),
                    };
                    let transition = state.event(&ctx, &WindowEvent::Resized(new_size));
                    self.apply_transition(transition, event_loop);
                }
            }
            WindowEvent::RedrawRequested => {
                let Some(gpu) = &self.gpu else { return };
                let Some(renderer) = &mut self.renderer else { return };
                let Some(state) = &mut self.state else { return };

                let now = Instant::now();
                let dt = self
                    .last_frame_time
                    .map(|t| now.duration_since(t).as_secs_f32())
                    .unwrap_or(1.0 / 60.0);
                self.last_frame_time = Some(now);

                let update_ctx = UpdateContext {
                    device: &gpu.device,
                    queue: &gpu.queue,
                    viewport_size: (gpu.config.width, gpu.config.height),
                    dt,
                };
                let transition = state.update(&update_ctx);
                match transition {
                    StateTransition::Continue => {}
                    StateTransition::Switch(new_state) => {
                        *state = new_state;
                    }
                    StateTransition::Quit => {
                        event_loop.exit();
                        return;
                    }
                }

                let surface_texture = match gpu.surface.get_current_texture() {
                    Ok(tex) => tex,
                    Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                        tracing::warn!("Surface lost or outdated, reconfiguring");
                        gpu.surface.configure(&gpu.device, &gpu.config);
                        return;
                    }
                    Err(wgpu::SurfaceError::Timeout) => {
                        tracing::warn!("Surface acquire timed out, skipping frame");
                        return;
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to acquire surface texture");
                        return;
                    }
                };

                let view = surface_texture
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());

                let mut render_ctx = RenderContext {
                    renderer,
                    device: &gpu.device,
                    queue: &gpu.queue,
                    view: &view,
                    viewport_size: (gpu.config.width, gpu.config.height),
                };
                state.render(&mut render_ctx);

                if self.overlay_state.is_visible() {
                    if let Some(egui) = &mut self.egui {
                        let window = self.window.as_ref().unwrap();
                        egui.render(
                            window,
                            &gpu.device,
                            &gpu.queue,
                            &view,
                            gpu.config.width,
                            gpu.config.height,
                            |ctx| state.overlay_ui(ctx),
                        );
                    }
                }

                surface_texture.present();
            }
            other => {
                if is_escape_press(&other) {
                    self.overlay_state.toggle();
                    return;
                }

                let egui_consumed = if let Some(egui) = &mut self.egui {
                    let window = self.window.as_ref().unwrap();
                    let response = egui.handle_event(window, &other);
                    response.consumed && self.overlay_state.is_visible()
                } else {
                    false
                };

                if !egui_consumed {
                    if let Some(state) = &mut self.state {
                        let gpu = self.gpu.as_ref();
                        let viewport_size = gpu
                            .map(|g| (g.config.width, g.config.height))
                            .unwrap_or((0, 0));
                        let ctx = EventContext { viewport_size };
                        let transition = state.event(&ctx, &other);
                        self.apply_transition(transition, event_loop);
                    }
                }
            }
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

pub fn run(factory: impl FnOnce(&InitContext) -> Box<dyn AppState> + 'static) {
    tracing::info!("Creating event loop");
    let event_loop = EventLoop::new().expect("failed to create event loop");
    let mut app = App::new(Box::new(factory));
    event_loop.run_app(&mut app).expect("event loop error");
}
