pub mod fps_counter;
pub mod game;
pub mod gfx;
pub mod menu;
pub mod physics;
pub mod renderer;
pub mod resources;
pub mod starfield;
pub mod state_machine;

use anyhow::Result;
use winit::event_loop::EventLoop;

use crate::game::GameState;
use crate::state_machine::{App, State};

pub fn run() -> Result<()> {
    let event_loop = EventLoop::new()?;
    let mut app = App::new(false, |gfx| {
        GameState::new(gfx).map(|s| Box::new(s) as Box<dyn State>)
    });
    event_loop.run_app(&mut app)?;
    Ok(())
}

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn wasm_main() {
    console_error_panic_hook::set_once();
    tracing_wasm::set_as_global_default();
    {
        use wasm_bindgen::JsCast;
        if let Some(canvas) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("canvas"))
            .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok())
        {
            canvas.set_width(crate::game::GAME_WIDTH);
            canvas.set_height(crate::game::GAME_HEIGHT);
        }
    }
    let event_loop = EventLoop::new().expect("event loop");
    let app = App::new(true, |gfx| {
        GameState::new(gfx).map(|s| Box::new(s) as Box<dyn State>)
    });
    use winit::platform::web::EventLoopExtWebSys;
    event_loop.spawn_app(app);
}
