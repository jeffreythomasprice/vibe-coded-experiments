#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

#[cfg(target_arch = "wasm32")]
mod app;
mod camera;
mod config;
mod error;
mod game;
#[cfg(target_arch = "wasm32")]
mod gpu;
mod hex;
mod icon;
mod info;
mod input;
mod prim;
mod resource;
mod scene;
mod text;
mod toast;
mod ui;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    console_log::init_with_level(log::Level::Info)
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    app::run().inspect_err(|e| log::error!("{e}"))?;
    Ok(())
}
