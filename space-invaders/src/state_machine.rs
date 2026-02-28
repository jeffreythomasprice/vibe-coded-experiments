use std::sync::Arc;

use anyhow::Result;
use tracing::*;
use web_time::Instant;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::ActiveEventLoop,
    window::{Window, WindowId},
};

pub enum StateSwitch {
    NoChange,
    ExitApplication,
    SwitchToState(Box<dyn State>),
}

pub trait State {
    fn update(&mut self, dt: std::time::Duration) -> Result<()>;
    fn render(&mut self, gfx: &GfxState) -> Result<()>;
    fn handle_event(&mut self, event: WindowEvent) -> Result<StateSwitch>;
}

pub struct GfxState {
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
}

impl GfxState {
    async fn new(window: Arc<Window>, vsync: bool) -> Result<Self> {
        let size = window.inner_size();

        #[cfg(target_arch = "wasm32")]
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::GL,
            ..Default::default()
        });
        #[cfg(not(target_arch = "wasm32"))]
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window)?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| anyhow::anyhow!("No suitable GPU adapter found"))?;

        #[cfg(not(target_arch = "wasm32"))]
        let limits = wgpu::Limits::default();
        #[cfg(target_arch = "wasm32")]
        let limits = wgpu::Limits::downlevel_webgl2_defaults();

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::empty(),
                    required_limits: limits,
                    memory_hints: wgpu::MemoryHints::default(),
                },
                None,
            )
            .await?;

        let caps = surface.get_capabilities(&adapter);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: caps.formats[0],
            width: size.width,
            height: size.height,
            present_mode: if vsync {
                wgpu::PresentMode::AutoVsync
            } else {
                wgpu::PresentMode::AutoNoVsync
            },
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        if config.width > 0 && config.height > 0 {
            surface.configure(&device, &config);
        }

        info!("GPU: {}", adapter.get_info().name);

        Ok(Self {
            surface,
            device,
            queue,
            config,
        })
    }

    fn resize(&mut self, size: winit::dpi::PhysicalSize<u32>) {
        if size.width > 0 && size.height > 0 {
            self.config.width = size.width;
            self.config.height = size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }
}

pub struct App {
    window: Option<Arc<Window>>,
    gfx: Option<GfxState>,
    factory: Option<Box<dyn FnOnce(&GfxState) -> Result<Box<dyn State>>>>,
    state: Option<Box<dyn State>>,
    vsync: bool,
    last_frame: Instant,
    #[cfg(target_arch = "wasm32")]
    init_cell: std::rc::Rc<std::cell::RefCell<Option<(GfxState, Box<dyn State>)>>>,
}

impl App {
    pub fn new(
        vsync: bool,
        factory: impl FnOnce(&GfxState) -> Result<Box<dyn State>> + 'static,
    ) -> Self {
        Self {
            window: None,
            gfx: None,
            factory: Some(Box::new(factory)),
            state: None,
            vsync,
            last_frame: Instant::now(),
            #[cfg(target_arch = "wasm32")]
            init_cell: std::rc::Rc::new(std::cell::RefCell::new(None)),
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        #[cfg(target_arch = "wasm32")]
        let window = {
            use wasm_bindgen::JsCast;
            use winit::platform::web::WindowAttributesExtWebSys;
            let canvas = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.get_element_by_id("canvas"))
                .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok())
                .expect("canvas#canvas not found");
            Arc::new(
                event_loop
                    .create_window(
                        Window::default_attributes()
                            .with_title("Space Invaders")
                            .with_canvas(Some(canvas)),
                    )
                    .unwrap(),
            )
        };

        #[cfg(not(target_arch = "wasm32"))]
        let window = Arc::new(
            event_loop
                .create_window(Window::default_attributes().with_title("Space Invaders"))
                .unwrap_or_else(|e| {
                    error!("failed to create window: {e}");
                    panic!("failed to create window");
                }),
        );

        self.window = Some(window.clone());

        let Some(factory) = self.factory.take() else {
            error!("resumed called after factory already consumed");
            panic!("resumed called twice");
        };

        #[cfg(not(target_arch = "wasm32"))]
        {
            let gfx = pollster::block_on(GfxState::new(window, self.vsync)).unwrap_or_else(|e| {
                error!("failed to init wgpu: {e}");
                panic!("failed to init wgpu");
            });
            match factory(&gfx) {
                Ok(state) => self.state = Some(state),
                Err(e) => {
                    error!("failed to create initial state: {e}");
                    panic!("failed to create initial state");
                }
            }
            self.gfx = Some(gfx);
        }

        #[cfg(target_arch = "wasm32")]
        {
            let cell = self.init_cell.clone();
            let vsync = self.vsync;
            wasm_bindgen_futures::spawn_local(async move {
                match GfxState::new(window, vsync).await {
                    Ok(gfx) => match factory(&gfx) {
                        Ok(state) => *cell.borrow_mut() = Some((gfx, state)),
                        Err(e) => error!("failed to create state: {e}"),
                    },
                    Err(e) => error!("failed to init wgpu: {e}"),
                }
            });
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        #[cfg(target_arch = "wasm32")]
        if self.gfx.is_none() {
            if let Some((gfx, state)) = self.init_cell.borrow_mut().take() {
                self.gfx = Some(gfx);
                self.state = Some(state);
            }
        }

        if let WindowEvent::Resized(size) = event {
            if let Some(gfx) = &mut self.gfx {
                gfx.resize(size);
            }
        }

        if let WindowEvent::RedrawRequested = event {
            let now = Instant::now();
            let dt = now.duration_since(self.last_frame);
            self.last_frame = now;
            if let Some(state) = &mut self.state {
                if let Err(e) = state.update(dt) {
                    error!("update error: {e}");
                }
            }
            if let (Some(gfx), Some(state)) = (&self.gfx, &mut self.state) {
                if gfx.config.width > 0 && gfx.config.height > 0 {
                    if let Err(e) = state.render(gfx) {
                        error!("render error: {e}");
                    }
                }
            }
            if let Some(window) = &self.window {
                window.request_redraw();
            }
            return;
        }

        if let Some(state) = &mut self.state {
            match state.handle_event(event) {
                Ok(StateSwitch::NoChange) => {}
                Ok(StateSwitch::ExitApplication) => event_loop.exit(),
                Ok(StateSwitch::SwitchToState(new_state)) => self.state = Some(new_state),
                Err(e) => error!("handle_event error: {e}"),
            }
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}
