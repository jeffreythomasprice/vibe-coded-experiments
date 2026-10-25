use wasm_bindgen::JsCast;
use web_sys::HtmlCanvasElement;
use web_time::Instant;
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{
    ElementState, KeyEvent, MouseButton as WinitButton, MouseScrollDelta, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, NamedKey};
use winit::platform::web::{EventLoopExtWebSys, WindowAttributesExtWebSys, WindowExtWebSys};
use winit::window::{CursorIcon, Window, WindowAttributes, WindowId};

use crate::camera::Camera;
use crate::config::game::{HEX_SIZE, MAX_FRAME_DT};
use crate::config::input::{InputConfig, WHEEL_DELTA_UNIT};
use crate::config::theme::{self, Theme};
use crate::error::AppError;
use crate::game::Game;
use crate::gpu::{Frame, Gpu};
use crate::hex::{Axial, Layout, Point};
use crate::icon;
use crate::info;
use crate::input::{Command, Input, MouseButton, RawInput};
use crate::prim::Rect;
use crate::resource::Resource;
use crate::scene;
use crate::text::Fonts;
use crate::toast::Toasts;
use crate::ui::{self, UiInput};

const CANVAS_ID: &str = "canvas";
const WHEEL_LINE_UNIT: f64 = WHEEL_DELTA_UNIT / 3.0;

enum UserEvent {
    GpuReady(Result<Gpu, AppError>),
}

pub fn run() -> Result<(), AppError> {
    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let app = App::new(event_loop.create_proxy())?;
    event_loop.spawn_app(app);
    Ok(())
}

struct App {
    proxy: EventLoopProxy<UserEvent>,
    window: Option<Window>,
    gpu: Option<Gpu>,
    fonts: Fonts,
    camera: Camera,
    layout: Layout,
    game: Game,
    theme: &'static Theme,
    selected: Option<Axial>,
    input: Input,
    toasts: Toasts,
    cursor: Option<Point>,
    viewport: Point,
    scale: f64,
    last_frame: Option<Instant>,
    dirty: bool,
    sidebar: Option<Rect>,
    icon_hits: Vec<(Rect, Resource)>,
    sidebar_press: Option<Option<Resource>>,
    sidebar_scroll: f64,
    max_scroll: f64,
    hovering_icon: bool,
}

impl App {
    fn new(proxy: EventLoopProxy<UserEvent>) -> Result<Self, AppError> {
        Ok(App {
            proxy,
            window: None,
            gpu: None,
            fonts: Fonts::load()?,
            camera: Camera::default(),
            layout: Layout::new(HEX_SIZE),
            game: Game::new(js_sys::Math::random().to_bits()),
            theme: &theme::DARK,
            selected: None,
            input: Input::new(InputConfig::default()),
            toasts: Toasts::default(),
            cursor: None,
            viewport: Point::default(),
            scale: 1.0,
            last_frame: None,
            dirty: true,
            sidebar: None,
            icon_hits: Vec::new(),
            sidebar_press: None,
            sidebar_scroll: 0.0,
            max_scroll: 0.0,
            hovering_icon: false,
        })
    }

    fn create_window(&mut self, event_loop: &ActiveEventLoop) -> Result<(), AppError> {
        let canvas = web_sys::window()
            .ok_or(AppError::NoWindow)?
            .document()
            .ok_or(AppError::NoDocument)?
            .get_element_by_id(CANVAS_ID)
            .ok_or(AppError::ElementNotFound(CANVAS_ID))?
            .dyn_into::<HtmlCanvasElement>()
            .map_err(|_| AppError::NotACanvas(CANVAS_ID))?;
        let window = event_loop.create_window(
            WindowAttributes::default()
                .with_canvas(Some(canvas))
                .with_append(false),
        )?;
        let surface_canvas = window.canvas().ok_or(AppError::NotACanvas(CANVAS_ID))?;
        if let Err(e) = surface_canvas.focus() {
            log::debug!("canvas focus failed: {e:?}");
        }
        self.scale = window.scale_factor();
        self.viewport = logical(window.inner_size(), self.scale);
        let size = window.inner_size();
        let proxy = self.proxy.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let gpu = Gpu::new(surface_canvas, size.width, size.height).await;
            if proxy.send_event(UserEvent::GpuReady(gpu)).is_err() {
                log::error!("event loop closed before the gpu was ready");
            }
        });
        self.window = Some(window);
        Ok(())
    }

    fn resize(&mut self, size: PhysicalSize<u32>) {
        let Some(window) = &self.window else {
            return;
        };
        self.scale = window.scale_factor();
        self.viewport = logical(size, self.scale);
        if let Some(gpu) = &mut self.gpu {
            gpu.resize(size.width, size.height);
        }
        self.dirty = true;
    }

    fn to_logical(&self, p: PhysicalPosition<f64>) -> Point {
        Point::new(p.x / self.scale, p.y / self.scale)
    }

    fn over_sidebar(&self, p: Point) -> bool {
        self.sidebar.is_some_and(|r| r.contains(p))
    }

    fn icon_at(&self, p: Point) -> Option<Resource> {
        self.icon_hits
            .iter()
            .find(|(rect, _)| rect.contains(p))
            .map(|&(_, r)| r)
    }

    fn zoom_at(&mut self, screen: Point, factor: f64) {
        self.camera.zoom_at(screen, self.viewport, factor);
    }

    fn on_input(&mut self, raw: RawInput) {
        if matches!(raw, RawInput::Blur) {
            self.cursor = None;
        }
        for command in self.input.handle(raw) {
            self.apply(command);
        }
        self.dirty = true;
    }

    fn apply(&mut self, command: Command) {
        match command {
            Command::Select(p) => self.select(p),
            Command::Gather(p) => self.gather_at(p),
            Command::Deselect => self.selected = None,
            Command::PanBy(delta) => self.camera.pan_by_screen(delta),
            Command::ZoomAt(p, factor) => self.zoom_at(p, factor),
            Command::ZoomAtCenter(factor) => self.zoom_at(self.viewport * 0.5, factor),
        }
        self.dirty = true;
    }

    fn gather_at(&mut self, screen: Point) {
        let pos = self.hex_at(screen);
        let world = self.camera.screen_to_world(screen, self.viewport);
        let Some(cell) = self.game.cell(pos) else {
            return;
        };
        let picked = cell.sole_resource().or_else(|| {
            let slot = icon::icon_at(self.layout, pos, cell.resources().count(), world)?;
            cell.resources().nth(slot).map(|(r, _, _)| r)
        });
        if let Some(r) = picked {
            self.gather(pos, r, screen);
        }
    }

    fn gather_selected(&mut self, resource: Resource, at: Point) {
        if let Some(pos) = self.selected {
            self.gather(pos, resource, at);
            self.dirty = true;
        }
    }

    fn gather(&mut self, pos: Axial, resource: Resource, at: Point) {
        match self.game.gather(pos, resource) {
            Ok(0) => {}
            Ok(amount) => {
                if let Some(icon) = self.game.cell(pos).and_then(|c| c.icon(resource)) {
                    self.toasts.push(resource, amount, icon.clone(), at);
                }
            }
            Err(e) => log::debug!("gather rejected: {e}"),
        }
    }

    fn hex_at(&self, screen: Point) -> Axial {
        let world = self.camera.screen_to_world(screen, self.viewport);
        self.layout.pixel_to_hex(world)
    }

    fn select(&mut self, screen: Point) {
        let pos = self.hex_at(screen);
        if self.game.is_owned(pos) {
            self.selected = Some(pos);
        } else if let Some(from) = self.selected {
            self.selected = match self.game.expand(from, pos) {
                Ok(()) => Some(pos),
                Err(e) => {
                    log::debug!("expand rejected: {e}");
                    None
                }
            };
        }
    }

    fn on_key(&mut self, event: &KeyEvent) {
        if event.repeat {
            return;
        }
        let Some(name) = key_name(&event.logical_key) else {
            return;
        };
        self.on_input(match event.state {
            ElementState::Pressed => RawInput::KeyDown(name),
            ElementState::Released => RawInput::KeyUp(name),
        });
    }

    fn on_cursor_moved(&mut self, position: PhysicalPosition<f64>) {
        let p = self.to_logical(position);
        self.cursor = Some(p);
        self.on_input(RawInput::MouseMove(p));
        let over_icon = self.icon_at(p).is_some();
        if over_icon != self.hovering_icon {
            self.hovering_icon = over_icon;
            if let Some(window) = &self.window {
                window.set_cursor(if over_icon {
                    CursorIcon::Pointer
                } else {
                    CursorIcon::Default
                });
            }
        }
    }

    fn on_mouse_button(&mut self, state: ElementState, button: WinitButton) {
        let Some(p) = self.cursor else {
            return;
        };
        let button = mouse_button(button);
        match state {
            ElementState::Pressed => {
                if self.over_sidebar(p) {
                    self.sidebar_press = (button == MouseButton::Left).then(|| self.icon_at(p));
                } else {
                    self.on_input(RawInput::MouseDown(button, p));
                }
            }
            ElementState::Released => {
                if button == MouseButton::Left
                    && let Some(pressed) = self.sidebar_press.take()
                    && let Some(resource) = pressed.filter(|&r| self.icon_at(p) == Some(r))
                {
                    self.gather_selected(resource, p);
                }
                self.on_input(RawInput::MouseUp(button, p));
            }
        }
    }

    fn on_wheel(&mut self, delta: MouseScrollDelta) {
        let Some(at) = self.cursor else {
            return;
        };
        let delta_y = match delta {
            MouseScrollDelta::LineDelta(_, y) => -f64::from(y) * WHEEL_LINE_UNIT,
            MouseScrollDelta::PixelDelta(p) => -p.y / self.scale,
        };
        if self.over_sidebar(at) {
            self.sidebar_scroll = (self.sidebar_scroll + delta_y).clamp(0.0, self.max_scroll);
            self.dirty = true;
        } else {
            self.on_input(RawInput::Wheel { delta_y, at });
        }
    }

    fn redraw(&mut self) {
        let now = Instant::now();
        let dt = self
            .last_frame
            .map_or(0.0, |last| (now - last).as_secs_f64().min(MAX_FRAME_DT));
        self.last_frame = Some(now);
        for command in self.input.frame(dt) {
            self.apply(command);
        }
        self.toasts.tick(dt, self.theme.toast.duration);
        self.dirty = false;

        let hover = self
            .cursor
            .filter(|&p| !self.input.is_dragging() && !self.over_sidebar(p))
            .map(|p| (self.hex_at(p), p))
            .and_then(|(pos, p)| Some((info::summary(&self.game, pos)?, p)));
        let details = self.selected.and_then(|pos| info::details(&self.game, pos));

        let ui = ui::build(
            &self.fonts,
            self.theme,
            self.viewport,
            &UiInput {
                hover: hover.as_ref().map(|(summary, p)| (summary, *p)),
                details: details.as_ref(),
                toasts: &self.toasts,
                sidebar_scroll: self.sidebar_scroll,
            },
        );
        self.max_scroll = ui.max_scroll;
        self.sidebar_scroll = self.sidebar_scroll.min(ui.max_scroll);
        self.sidebar = ui.sidebar;
        self.icon_hits.clone_from(&ui.icon_hits);

        let Some(gpu) = &mut self.gpu else {
            return;
        };
        let hexes = scene::hex_instances(
            &self.game,
            &self.camera,
            self.layout,
            self.viewport,
            self.theme,
            self.selected,
        );
        let world_icons = scene::world_icons(&self.game, &self.camera, self.layout, self.viewport);
        gpu.render(&Frame {
            grid: scene::grid_uniforms(&self.camera, self.layout, self.viewport, self.theme),
            hexes: &hexes,
            world_icons: &world_icons,
            ui: &ui,
            fonts: &self.fonts,
            scale: self.scale,
        });
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        if let Err(e) = self.create_window(event_loop) {
            log::error!("{e}");
        }
    }

    fn user_event(&mut self, _: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::GpuReady(Ok(gpu)) => {
                self.gpu = Some(gpu);
                if let Some(window) = &self.window {
                    self.resize(window.inner_size());
                }
                self.dirty = true;
            }
            UserEvent::GpuReady(Err(e)) => log::error!("{e}"),
        }
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::Resized(size) => self.resize(size),
            WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(window) = &self.window {
                    self.resize(window.inner_size());
                }
            }
            WindowEvent::RedrawRequested => self.redraw(),
            WindowEvent::KeyboardInput { event, .. } => self.on_key(&event),
            WindowEvent::CursorMoved { position, .. } => self.on_cursor_moved(position),
            WindowEvent::CursorLeft { .. } => {
                self.cursor = None;
                self.dirty = true;
            }
            WindowEvent::MouseInput { state, button, .. } => self.on_mouse_button(state, button),
            WindowEvent::MouseWheel { delta, .. } => self.on_wheel(delta),
            WindowEvent::Focused(false) => self.on_input(RawInput::Blur),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        let animating = self.dirty || !self.toasts.is_empty() || self.input.wants_frames();
        match (&self.window, animating) {
            (Some(window), true) => window.request_redraw(),
            _ => self.last_frame = None,
        }
    }
}

fn logical(size: PhysicalSize<u32>, scale: f64) -> Point {
    Point::new(
        f64::from(size.width) / scale,
        f64::from(size.height) / scale,
    )
}

fn mouse_button(button: WinitButton) -> MouseButton {
    match button {
        WinitButton::Left => MouseButton::Left,
        WinitButton::Middle => MouseButton::Middle,
        WinitButton::Right => MouseButton::Right,
        WinitButton::Back => MouseButton::Other(3),
        WinitButton::Forward => MouseButton::Other(4),
        WinitButton::Other(n) => MouseButton::Other(n),
    }
}

fn key_name(key: &Key) -> Option<String> {
    match key {
        Key::Character(s) => Some(s.to_lowercase()),
        Key::Named(NamedKey::ArrowLeft) => Some("arrowleft".into()),
        Key::Named(NamedKey::ArrowRight) => Some("arrowright".into()),
        Key::Named(NamedKey::ArrowUp) => Some("arrowup".into()),
        Key::Named(NamedKey::ArrowDown) => Some("arrowdown".into()),
        Key::Named(NamedKey::Escape) => Some("escape".into()),
        _ => None,
    }
}
