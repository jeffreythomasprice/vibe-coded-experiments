//! The winit glue for the shared [`ui`] menu.
//!
//! This is the only egui-and-winit-coupled piece, kept out of the reusable `ui`
//! crate: it owns the `egui::Context` and the `egui_winit::State` that translates
//! winit events into egui input and applies egui's platform output back. Each
//! frame it feeds [`ui::draw`] and tessellates the result into a
//! [`video::EguiPaint`] the GPU backend can render. A different frontend supplies
//! its own equivalent of this file.

use ui::{MenuCommand, MenuData, MenuState};
use video::EguiPaint;
use winit::window::Window;

pub struct EguiPlatform {
    ctx: egui::Context,
    state: egui_winit::State,
}

impl EguiPlatform {
    pub fn new(window: &Window) -> EguiPlatform {
        let ctx = egui::Context::default();
        ui::install_fonts(&ctx);
        let state = egui_winit::State::new(
            ctx.clone(),
            egui::ViewportId::ROOT,
            window,
            None,
            None,
            None,
        );
        EguiPlatform { ctx, state }
    }

    /// Feed a window event to egui. The returned `consumed` flag lets the host
    /// suppress the emulator input router when the UI used the event.
    pub fn on_window_event(
        &mut self,
        window: &Window,
        event: &winit::event::WindowEvent,
    ) -> egui_winit::EventResponse {
        self.state.on_window_event(window, event)
    }

    /// Build the menu for this frame: take egui input, run [`ui::draw`] against
    /// `menu`/`data`, apply platform output, and tessellate into an
    /// [`EguiPaint`]. Returns the paint job plus any commands the UI produced.
    pub fn run(
        &mut self,
        window: &Window,
        menu: &mut MenuState,
        data: &MenuData,
    ) -> (EguiPaint, Vec<MenuCommand>) {
        let raw_input = self.state.take_egui_input(window);
        let mut commands = Vec::new();
        let output = self.ctx.run(raw_input, |ctx| {
            commands = ui::draw(ctx, menu, data);
        });
        self.state
            .handle_platform_output(window, output.platform_output);
        let primitives = self.ctx.tessellate(output.shapes, output.pixels_per_point);
        let paint = EguiPaint {
            primitives,
            textures_delta: output.textures_delta,
            pixels_per_point: output.pixels_per_point,
        };
        (paint, commands)
    }
}
