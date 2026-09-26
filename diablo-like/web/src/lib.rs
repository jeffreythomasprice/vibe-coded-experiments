use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn wasm_main() {
    console_error_panic_hook::set_once();
    tracing_wasm::set_as_global_default();

    if let Err(err) = engine::run(
        wasm_bindgen_futures::spawn_local,
        engine::EngineConfig::default(),
    ) {
        tracing::error!("engine exited with error: {err}");
    }
}
