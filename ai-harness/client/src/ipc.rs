//! The bridge to the Tauri backend.
//!
//! `withGlobalTauri` is enabled in `tauri.conf.json`, so the invoke function is
//! reachable at `window.__TAURI__.core.invoke` without a JS package.

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], js_name = invoke)]
    pub async fn invoke(cmd: &str, args: JsValue) -> JsValue;
}
