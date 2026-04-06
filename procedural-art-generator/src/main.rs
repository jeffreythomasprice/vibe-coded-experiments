pub mod camera;
mod egui_integration;
pub mod fps;
pub(crate) mod graphics;
mod menu;
pub mod parameterized_grid;
pub mod state;
mod windowing;

use camera::{Camera2D, Rect};
use fps::FpsCounter;
use glam::Vec2;
use graphics::font::{HAlign, TextureAtlasFont, VAlign};
use graphics::immediate::Color;
use menu::MenuState;
use parameterized_grid::ArtGeneratorGrid;
use state::{
    AppState, EventContext, InitContext, InputState, RenderContext, StateTransition, UpdateContext,
};
use winit::event::WindowEvent;

fn main() {
    init_tracing();
    tracing::info!("Starting up");
    windowing::run(|ctx: &InitContext| {
        let font_data = include_bytes!("../assets/IntelOneMono-Regular.ttf");
        let font = TextureAtlasFont::from_ascii(ctx.device, ctx.queue, font_data, 24.0);
        let aspect = ctx.viewport_size.0 as f32 / ctx.viewport_size.1 as f32;
        let world_bounds = Rect::new(Vec2::new(-200.0, -200.0), Vec2::new(200.0, 200.0));
        let grid = ArtGeneratorGrid::new(
            0,
            8,
            4,
            world_bounds,
            2.0,
            None,
            ctx.device,
            ctx.queue,
        );
        let menu_state = MenuState::new(grid.rows(), grid.cols(), &grid.param_ranges());
        let camera = Camera2D::new(world_bounds, world_bounds, 50.0, aspect);
        Box::new(DemoState {
            camera,
            font,
            fps: FpsCounter::new(),
            input: InputState::new(),
            grid,
            menu_state,
        })
    });
}

struct DemoState {
    camera: Camera2D,
    font: TextureAtlasFont,
    fps: FpsCounter,
    input: InputState,
    grid: ArtGeneratorGrid,
    menu_state: MenuState,
}

impl AppState for DemoState {
    fn event(&mut self, _ctx: &EventContext, event: &WindowEvent) -> StateTransition {
        self.input.handle_event(event);

        match event {
            WindowEvent::Resized(new_size) => {
                if new_size.width > 0 && new_size.height > 0 {
                    self.camera
                        .resize(new_size.width as f32 / new_size.height as f32);
                }
            }
            _ => {}
        }

        StateTransition::Continue
    }

    fn update(&mut self, ctx: &UpdateContext) -> StateTransition {
        if self.menu_state.needs_generator_switch {
            self.menu_state.needs_generator_switch = false;
            self.menu_state.needs_regenerate = false;
            let bounds = self.grid.bounds();
            self.grid = ArtGeneratorGrid::new(
                self.menu_state.selected_generator,
                self.menu_state.pending_cols,
                self.menu_state.pending_rows,
                bounds,
                2.0,
                None,
                ctx.device,
                ctx.queue,
            );
            self.menu_state.pending_ranges = self.grid.param_ranges();
        } else if self.menu_state.needs_regenerate {
            self.menu_state.needs_regenerate = false;
            let bounds = self.grid.bounds();
            let ranges = self.menu_state.pending_ranges.clone();
            self.grid = ArtGeneratorGrid::new(
                self.menu_state.selected_generator,
                self.menu_state.pending_cols,
                self.menu_state.pending_rows,
                bounds,
                2.0,
                Some(ranges),
                ctx.device,
                ctx.queue,
            );
        }

        let dt = ctx.dt;
        let h = ctx.viewport_size.1 as f32;

        let pan_speed = self.camera.camera_height() * 1.5;
        let mut pan = Vec2::ZERO;
        if self.input.is_held("w") {
            pan.y -= 1.0;
        }
        if self.input.is_held("s") {
            pan.y += 1.0;
        }
        if self.input.is_held("a") {
            pan.x -= 1.0;
        }
        if self.input.is_held("d") {
            pan.x += 1.0;
        }
        if pan != Vec2::ZERO {
            self.camera.pan(pan.normalize() * pan_speed * dt);
        }

        if self.input.right_mouse_held {
            let delta_screen = self.input.cursor_pos - self.input.prev_cursor_pos;
            let scale = self.camera.camera_height() / h;
            self.camera.pan(Vec2::new(
                -delta_screen.x * scale * self.camera.aspect(),
                -delta_screen.y * scale,
            ));
        }

        let zoom_speed: f32 = 2.0;
        if self.input.is_held("=") {
            self.camera.zoom(zoom_speed.powf(dt), None);
        }
        if self.input.is_held("-") {
            self.camera.zoom(zoom_speed.powf(-dt), None);
        }

        if self.input.scroll_delta != 0.0 {
            let factor = 1.1_f32.powf(self.input.scroll_delta);
            let viewport_size = Vec2::new(ctx.viewport_size.0 as f32, ctx.viewport_size.1 as f32);
            let anchor = self
                .camera
                .screen_to_world(self.input.cursor_pos, viewport_size);
            self.camera.zoom(factor, Some(anchor));
        }

        let viewport_size = Vec2::new(ctx.viewport_size.0 as f32, ctx.viewport_size.1 as f32);
        let world_pos = self
            .camera
            .screen_to_world(self.input.cursor_pos, viewport_size);
        self.grid.update_hover(world_pos);

        self.grid.update_time(dt);
        self.grid.poll_inits();

        self.input.end_frame();
        StateTransition::Continue
    }

    fn render(&mut self, ctx: &mut RenderContext) {
        let (w, h) = (ctx.viewport_size.0 as f32, ctx.viewport_size.1 as f32);

        // Pass 1: World content
        {
            let mut frame = ctx.renderer.begin(
                ctx.device,
                ctx.queue,
                ctx.view,
                self.camera.uniforms(),
                Some(Color::CORNFLOWER_BLUE.into()),
            );

            self.grid.render_cells(&mut frame);
            if let Some(outline) = self.grid.draw_hover_outline() {
                let mut mat = frame.color_material(Color::WHITE, glam::Mat4::IDENTITY);
                mat.indexed_triangle_list(&outline.vertices, &outline.indices);
            }
        }

        // Pass 2: UI overlay (screen-space)
        {
            let mut frame = ctx.renderer.begin(
                ctx.device,
                ctx.queue,
                ctx.view,
                Camera2D::screen_uniforms(w, h),
                None,
            );

            self.fps.tick();

            let mut tooltip_text = None;
            if let Some(descs) = self.grid.hover_description() {
                let text: String = descs
                    .iter()
                    .map(|d| format!("{}: {}", d.name, d.value))
                    .collect::<Vec<_>>()
                    .join("\n");
                tooltip_text = Some(text);
            }

            let mut mat = self.font.material(&mut frame);
            let fps_text = format!("{:.1} FPS", self.fps.fps());
            mat.draw(
                &fps_text,
                Vec2::new(0., 0.),
                Vec2::new(w, h),
                HAlign::Left,
                VAlign::Top,
            );

            if let Some(text) = &tooltip_text {
                let padding = 16.0;
                let text_size = self.font.measure(text);
                let mut pos = self.input.cursor_pos + Vec2::new(padding, padding);

                if pos.x + text_size.x > w {
                    pos.x = (self.input.cursor_pos.x - padding - text_size.x).max(0.0);
                }
                if pos.y + text_size.y > h {
                    pos.y = (self.input.cursor_pos.y - padding - text_size.y).max(0.0);
                }

                mat.draw(text, pos, text_size, HAlign::Left, VAlign::Top);
            }
        }
    }

    fn overlay_ui(&mut self, ctx: &egui::Context) {
        let defs = self.grid.param_defs();
        menu::draw_menu(ctx, &mut self.menu_state, &defs);
    }
}

fn init_tracing() {
    use tracing_subscriber::EnvFilter;

    let pkg_name = env!("CARGO_PKG_NAME").replace('-', "_");
    let default_filter = format!("warn,{pkg_name}=trace");

    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&default_filter));

    tracing_subscriber::fmt().with_env_filter(filter).init();
}
