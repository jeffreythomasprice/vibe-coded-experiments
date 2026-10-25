use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::convert::FromWasmAbi;
use wasm_bindgen::prelude::*;
use web_sys::{AddEventListenerOptions, EventTarget, KeyboardEvent, MouseEvent, WheelEvent};

use crate::camera::Camera;
use crate::canvas::Canvas;
use crate::config::game::{HEX_SIZE, MAX_FRAME_DT};
use crate::config::input::InputConfig;
use crate::config::theme::{self, Theme};
use crate::error::AppError;
use crate::game::Game;
use crate::hex::{Axial, Layout, Point};
use crate::icon;
use crate::input::{Command, Input, RawInput};
use crate::overlay::{self, Overlay};
use crate::render;
use crate::resource::Resource;
use crate::toast::Toasts;

pub struct App {
    canvas: Canvas,
    camera: Camera,
    layout: Layout,
    game: Game,
    theme: &'static Theme,
    selected: Option<Axial>,
    input: Input,
    overlay: Overlay,
    toasts: Toasts,
    cursor: Option<Point>,
    last_frame: Option<f64>,
    dirty: bool,
}

type Shared = Rc<RefCell<App>>;
type FrameCallback = Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>>;

impl App {
    pub fn start() -> Result<(), AppError> {
        let window = web_sys::window().ok_or(AppError::NoWindow)?;
        let app: Shared = Rc::new(RefCell::new(App {
            canvas: Canvas::from_element_id("canvas")?,
            camera: Camera::default(),
            layout: Layout::new(HEX_SIZE),
            game: Game::new(js_sys::Math::random().to_bits()),
            theme: &theme::DARK,
            selected: None,
            input: Input::new(InputConfig::default()),
            overlay: Overlay::new(&theme::DARK)?,
            toasts: Toasts::default(),
            cursor: None,
            last_frame: None,
            dirty: true,
        }));

        let canvas_target: EventTarget = window
            .document()
            .ok_or(AppError::NoDocument)?
            .get_element_by_id("canvas")
            .ok_or(AppError::ElementNotFound("canvas"))?
            .into();

        let a = Rc::clone(&app);
        listen(&canvas_target, "mousedown", move |e: MouseEvent| {
            a.borrow_mut()
                .on_input(RawInput::MouseDown(e.button().into(), mouse_point(&e)));
        })?;

        let a = Rc::clone(&app);
        listen(&canvas_target, "mousemove", move |e: MouseEvent| {
            a.borrow_mut().cursor = Some(mouse_point(&e));
        })?;

        let a = Rc::clone(&app);
        listen(&canvas_target, "mouseleave", move |_: web_sys::Event| {
            a.borrow_mut().cursor = None;
        })?;

        let a = Rc::clone(&app);
        let sidebar_target = app.borrow().overlay.sidebar_target();
        listen(&sidebar_target, "click", move |e: MouseEvent| {
            if let Some(r) = overlay::sidebar_resource(&e) {
                a.borrow_mut().gather_selected(r, mouse_point(&e));
            }
        })?;

        let a = Rc::clone(&app);
        listen(&window, "mousemove", move |e: MouseEvent| {
            a.borrow_mut()
                .on_input(RawInput::MouseMove(mouse_point(&e)));
        })?;

        let a = Rc::clone(&app);
        listen(&window, "mouseup", move |e: MouseEvent| {
            a.borrow_mut()
                .on_input(RawInput::MouseUp(e.button().into(), mouse_point(&e)));
        })?;

        let a = Rc::clone(&app);
        let options = AddEventListenerOptions::new();
        options.set_passive(false);
        let wheel = Closure::<dyn FnMut(WheelEvent)>::new(move |e: WheelEvent| {
            e.prevent_default();
            a.borrow_mut().on_input(RawInput::Wheel {
                delta_y: e.delta_y(),
                at: mouse_point(&e),
            });
        });
        canvas_target.add_event_listener_with_callback_and_add_event_listener_options(
            "wheel",
            wheel.as_ref().unchecked_ref(),
            &options,
        )?;
        wheel.forget();

        let a = Rc::clone(&app);
        listen(&window, "keydown", move |e: KeyboardEvent| {
            a.borrow_mut().on_input(RawInput::KeyDown(e.key()));
        })?;

        let a = Rc::clone(&app);
        listen(&window, "keyup", move |e: KeyboardEvent| {
            a.borrow_mut().on_input(RawInput::KeyUp(e.key()));
        })?;

        let a = Rc::clone(&app);
        listen(&window, "blur", move |_: web_sys::Event| {
            a.borrow_mut().on_input(RawInput::Blur);
        })?;

        let a = Rc::clone(&app);
        listen(&window, "resize", move |_: web_sys::Event| {
            let mut app = a.borrow_mut();
            if let Err(e) = app.canvas.resize_to_window() {
                log::error!("{e}");
            }
            app.dirty = true;
        })?;

        start_frame_loop(app)
    }

    fn viewport(&self) -> Point {
        Point::new(self.canvas.width(), self.canvas.height())
    }

    fn zoom_at(&mut self, screen: Point, factor: f64) {
        let viewport = self.viewport();
        self.camera.zoom_at(screen, viewport, factor);
    }

    fn on_input(&mut self, raw: RawInput) {
        if matches!(raw, RawInput::Blur) {
            self.cursor = None;
        }
        for command in self.input.handle(raw) {
            self.apply(command);
        }
    }

    fn apply(&mut self, command: Command) {
        match command {
            Command::Select(p) => self.select(p),
            Command::Gather(p) => self.gather_at(p),
            Command::Deselect => self.selected = None,
            Command::PanBy(delta) => self.camera.pan_by_screen(delta),
            Command::ZoomAt(p, factor) => self.zoom_at(p, factor),
            Command::ZoomAtCenter(factor) => self.zoom_at(self.viewport() * 0.5, factor),
        }
        self.dirty = true;
    }

    fn gather_at(&mut self, screen: Point) {
        let pos = self.hex_at(screen);
        let world = self.camera.screen_to_world(screen, self.viewport());
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
                    self.toasts.push(resource, amount, icon.to_svg(), at);
                }
            }
            Err(e) => log::debug!("gather rejected: {e}"),
        }
    }

    fn hex_at(&self, screen: Point) -> Axial {
        let world = self.camera.screen_to_world(screen, self.viewport());
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

    fn step(&mut self, dt: f64) {
        for command in self.input.frame(dt) {
            self.apply(command);
        }
    }

    fn frame(&mut self, now_ms: f64) -> Result<(), AppError> {
        let dt = self
            .last_frame
            .map_or(0.0, |last| ((now_ms - last) / 1000.0).min(MAX_FRAME_DT));
        self.last_frame = Some(now_ms);
        self.step(dt);
        self.toasts.tick(dt, self.theme.toast.duration);
        self.overlay.sync_toasts(&self.toasts, &self.theme.toast)?;
        if self.dirty {
            self.dirty = false;
            render::draw(
                &self.canvas,
                &self.camera,
                self.layout,
                &self.game,
                self.theme,
                self.selected,
            )?;
        }
        let hover = self
            .cursor
            .filter(|_| !self.input.is_dragging())
            .map(|p| (self.hex_at(p), p))
            .filter(|&(pos, _)| self.game.is_owned(pos));
        let viewport = self.viewport();
        self.overlay.set_hover(hover, &self.game, viewport)?;
        self.overlay.set_selected(self.selected, &self.game)?;
        Ok(())
    }
}

fn mouse_point(e: &MouseEvent) -> Point {
    Point::new(f64::from(e.client_x()), f64::from(e.client_y()))
}

fn listen<E: FromWasmAbi + 'static>(
    target: &EventTarget,
    name: &str,
    handler: impl FnMut(E) + 'static,
) -> Result<(), AppError> {
    let closure = Closure::<dyn FnMut(E)>::new(handler);
    target.add_event_listener_with_callback(name, closure.as_ref().unchecked_ref())?;
    closure.forget();
    Ok(())
}

fn start_frame_loop(app: Shared) -> Result<(), AppError> {
    let window = web_sys::window().ok_or(AppError::NoWindow)?;
    let slot: FrameCallback = Rc::new(RefCell::new(None));
    let next = Rc::clone(&slot);
    let window_for_loop = window.clone();
    *slot.borrow_mut() = Some(Closure::new(move |now: f64| {
        if let Err(e) = app.borrow_mut().frame(now) {
            log::error!("{e}");
        }
        if let Some(cb) = next.borrow().as_ref()
            && let Err(e) = window_for_loop.request_animation_frame(cb.as_ref().unchecked_ref())
        {
            log::error!("{}", AppError::from(e));
        }
    }));
    let first = slot.borrow();
    let cb = first.as_ref().expect("frame closure just stored");
    window.request_animation_frame(cb.as_ref().unchecked_ref())?;
    Ok(())
}
