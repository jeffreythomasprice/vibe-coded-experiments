mod app;
mod clip;
mod cuboid;
mod drag;
mod drill;
mod editor;
mod elements;
mod error;
mod gfx;
mod gizmo;
mod nav;
mod pane;
mod panels;
mod remote;
mod theme;
mod tree;

use std::path::PathBuf;

use error::AppError;
use winit::event_loop::{ControlFlow, EventLoop};

const DEFAULT_SCENE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../scenes/test.scene.toml");

fn main() -> Result<(), AppError> {
    let path = std::env::args().nth(1).map(PathBuf::from);
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = app::App::new(path, PathBuf::from(DEFAULT_SCENE));
    event_loop.run_app(&mut app)?;
    match app.take_error() {
        Some(e) => Err(e),
        None => Ok(()),
    }
}
