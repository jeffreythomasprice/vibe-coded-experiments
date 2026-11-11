use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::ViewportId;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::window::{Window, WindowId};

use crate::editor::{Editor, Timings};
use crate::error::AppError;
use crate::gfx::Gfx;
use crate::theme;

struct Running {
    gfx: Gfx,
    editor: Editor,
    ctx: egui::Context,
    egui_state: egui_winit::State,
}

pub struct App {
    path: Option<PathBuf>,
    default_scene: PathBuf,
    running: Option<Running>,
    error: Option<AppError>,
}

impl App {
    pub fn new(path: Option<PathBuf>, default_scene: PathBuf) -> Self {
        Self {
            path,
            default_scene,
            running: None,
            error: None,
        }
    }

    pub fn take_error(&mut self) -> Option<AppError> {
        self.error.take()
    }

    fn start(&mut self, event_loop: &ActiveEventLoop) -> Result<(), AppError> {
        let window = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title("Editor")
                    .with_inner_size(LogicalSize::new(1500.0, 900.0)),
            )?,
        );
        let gfx = Gfx::new(window.clone())?;
        let waker = window.clone();
        let editor = Editor::new(
            self.path.as_deref(),
            &self.default_scene,
            gfx.adapter.clone(),
            Box::new(move || waker.request_redraw()),
        )?;
        let ctx = egui::Context::default();
        theme::apply(&ctx, editor.theme);
        let egui_state = egui_winit::State::new(
            ctx.clone(),
            ViewportId::ROOT,
            &*window,
            Some(window.scale_factor() as f32),
            window.theme(),
            Some(gfx.device.limits().max_texture_dimension_2d as usize),
        );
        self.running = Some(Running {
            gfx,
            editor,
            ctx,
            egui_state,
        });
        window.request_redraw();
        Ok(())
    }

    fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        let Some(Running {
            gfx,
            editor,
            ctx,
            egui_state,
        }) = self.running.as_mut()
        else {
            return;
        };
        let started = Instant::now();
        let window = gfx.window.clone();
        let raw = egui_state.take_egui_input(&window);
        editor.repaint = false;
        let output = ctx.run_ui(raw, |ui| editor.ui(ui, &gfx.renderer));
        let ui_done = Instant::now();
        egui_state.handle_platform_output(&window, output.platform_output);
        editor.finish_frame(gfx);
        let panes_done = Instant::now();
        let jobs = ctx.tessellate(output.shapes, output.pixels_per_point);
        gfx.present(
            &jobs,
            output.textures_delta,
            output.pixels_per_point,
            theme::viewport_bg(editor.theme),
        );
        let ms = |a: Instant, b: Instant| b.duration_since(a).as_secs_f32() * 1000.0;
        let done = Instant::now();
        let t = &mut editor.timings;
        Timings::blend(&mut t.ui, ms(started, ui_done));
        Timings::blend(&mut t.panes, ms(ui_done, panes_done));
        Timings::blend(&mut t.present, ms(panes_done, done));
        Timings::blend(&mut t.frame, ms(started, done));

        if editor.quit {
            event_loop.exit();
            return;
        }
        let delay = output
            .viewport_output
            .get(&ViewportId::ROOT)
            .map_or(Duration::MAX, |v| v.repaint_delay);
        if editor.repaint || delay.is_zero() {
            window.request_redraw();
        } else if delay < Duration::from_secs(3600) {
            event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + delay));
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
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
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(running) = self.running.as_mut() else {
            return;
        };
        let response = running
            .egui_state
            .on_window_event(&running.gfx.window, &event);
        if response.repaint && !matches!(event, WindowEvent::RedrawRequested) {
            running.gfx.window.request_redraw();
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                running.gfx.resize(size.width, size.height);
                running.gfx.window.request_redraw();
            }
            WindowEvent::RedrawRequested => self.redraw(event_loop),
            _ => {}
        }
    }
}
