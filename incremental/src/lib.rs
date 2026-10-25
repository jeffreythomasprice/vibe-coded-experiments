mod app;
mod camera;
mod canvas;
mod config;
mod error;
mod game;
mod hex;
mod icon;
mod info;
mod input;
mod overlay;
mod render;
mod resource;
mod toast;

use wasm_bindgen::prelude::*;

use crate::app::App;

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    console_log::init_with_level(log::Level::Debug)
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    App::start().inspect_err(|e| log::error!("{e}"))?;
    Ok(())
}
