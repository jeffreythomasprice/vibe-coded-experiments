use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

use crate::error::GameError;
use crate::game::Game;
use crate::gfx::Gfx;
use crate::session::{Session, Target};

pub enum Source {
    Focused,
    Path(PathBuf),
    Id(String),
}

struct Running {
    gfx: Gfx,
    game: Game,
    captured: bool,
    title_at: Instant,
}

pub struct App {
    source: Source,
    default_scene: PathBuf,
    running: Option<Running>,
    error: Option<GameError>,
}

impl App {
    pub fn new(source: Source, default_scene: PathBuf) -> Self {
        Self {
            source,
            default_scene,
            running: None,
            error: None,
        }
    }

    pub fn take_error(&mut self) -> Option<GameError> {
        self.error.take()
    }

    fn start(&mut self, event_loop: &ActiveEventLoop) -> Result<(), GameError> {
        let window = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title("Game")
                    .with_inner_size(LogicalSize::new(1280.0, 720.0)),
            )?,
        );
        let gfx = Gfx::new(window.clone())?;
        let waker = window.clone();
        let target = match &self.source {
            Source::Focused => Target::Focused,
            Source::Path(p) => Target::Path(p),
            Source::Id(id) => Target::Id(id),
        };
        let session = Session::connect(
            target,
            &self.default_scene,
            Box::new(move || waker.request_redraw()),
        )?;
        let (game, warnings) = Game::new(session, gfx.size());
        for w in warnings {
            eprintln!("warning: {w}");
        }
        window.set_title(&game.title());
        self.running = Some(Running {
            gfx,
            game,
            captured: false,
            title_at: Instant::now(),
        });
        window.request_redraw();
        Ok(())
    }

    fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        let Some(running) = self.running.as_mut() else {
            return;
        };
        match running.game.update(&running.gfx) {
            Ok(warnings) => {
                for w in warnings {
                    eprintln!("warning: {w}");
                }
            }
            Err(e) => {
                self.error = Some(e);
                event_loop.exit();
                return;
            }
        }
        if running.game.closed() {
            eprintln!("the session was closed");
            event_loop.exit();
            return;
        }
        running.game.draw(&mut running.gfx);
        if running.title_at.elapsed() > Duration::from_millis(200) {
            running.gfx.window.set_title(&running.game.title());
            running.title_at = Instant::now();
        }
    }
}

fn capture(running: &mut Running, on: bool) {
    let window = &running.gfx.window;
    if on {
        let grabbed = window
            .set_cursor_grab(CursorGrabMode::Locked)
            .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined));
        if grabbed.is_ok() {
            window.set_cursor_visible(false);
            running.captured = true;
        }
    } else {
        let _ = window.set_cursor_grab(CursorGrabMode::None);
        window.set_cursor_visible(true);
        running.captured = false;
        running.game.input.clear();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.running.is_none()
            && let Err(e) = self.start(event_loop)
        {
            self.error = Some(e);
            event_loop.exit();
        }
        event_loop.set_control_flow(ControlFlow::Poll);
    }

    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        if let Some(running) = &self.running {
            running.gfx.window.request_redraw();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(running) = self.running.as_mut() else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => running.gfx.resize(size.width, size.height),
            WindowEvent::Focused(false) => capture(running, false),
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } if !running.captured => capture(running, true),
            WindowEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(code) = event.physical_key else {
                    return;
                };
                let down = event.state == ElementState::Pressed;
                running.game.input.key(code, down);
                if down && !event.repeat {
                    match code {
                        KeyCode::Escape if running.captured => capture(running, false),
                        KeyCode::Escape => event_loop.exit(),
                        KeyCode::KeyP => running.game.paused = !running.game.paused,
                        KeyCode::KeyR => running.game.restart(),
                        _ => {}
                    }
                }
            }
            WindowEvent::RedrawRequested => self.redraw(event_loop),
            _ => {}
        }
    }

    fn device_event(&mut self, _: &ActiveEventLoop, _: DeviceId, event: DeviceEvent) {
        if let (Some(running), DeviceEvent::MouseMotion { delta }) = (self.running.as_mut(), event)
            && running.captured
        {
            running.game.input.look.0 += delta.0;
            running.game.input.look.1 += delta.1;
        }
    }
}
