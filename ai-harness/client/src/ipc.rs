//! The bridge to the Tauri backend.
//!
//! `withGlobalTauri` is enabled in `tauri.conf.json`, so the invoke function is
//! reachable at `window.__TAURI__.core.invoke` without a JS package.

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    /// `catch` because a command that returns `Result::Err` rejects the JS
    /// promise — without it, that rejection would trap the wasm future
    /// instead of coming back as an `Err` here.
    #[wasm_bindgen(catch, js_namespace = ["window", "__TAURI__", "core"], js_name = invoke)]
    pub async fn invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
}
