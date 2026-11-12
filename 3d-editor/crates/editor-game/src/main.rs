mod app;
mod error;
mod game;
mod gfx;
mod input;
mod session;

use std::path::PathBuf;

use app::{App, Source};
use error::GameError;
use winit::event_loop::EventLoop;

const DEFAULT_SCENE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../scenes/demo-courtyard.scene.toml"
);

fn source() -> Result<Source, GameError> {
    let mut args = std::env::args().skip(1);
    match (args.next(), args.next(), args.next()) {
        (None, ..) => Ok(Source::Focused),
        (Some(flag), Some(id), None) if flag == "--session" => Ok(Source::Id(id)),
        (Some(path), None, _) if !path.starts_with("--") => Ok(Source::Path(PathBuf::from(path))),
        _ => Err(GameError::Usage),
    }
}

fn main() -> Result<(), GameError> {
    let source = source()?;
    let event_loop = EventLoop::new()?;
    let mut app = App::new(source, PathBuf::from(DEFAULT_SCENE));
    event_loop.run_app(&mut app)?;
    match app.take_error() {
        Some(e) => Err(e),
        None => Ok(()),
    }
}
