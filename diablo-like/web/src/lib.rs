use std::str::FromStr;

use tracing_subscriber::filter::Targets;
use tracing_subscriber::prelude::*;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn wasm_main() {
    console_error_panic_hook::set_once();

    let config = engine::config::Config::default();
    let (filter, origin) = config.logging.effective_filter();
    let (targets, parse_err) = match Targets::from_str(&filter) {
        Ok(targets) => (targets, None),
        Err(err) => (Targets::new().with_default(tracing::Level::WARN), Some(err)),
    };

    tracing_subscriber::registry()
        .with(tracing_wasm::WASMLayer::default().with_filter(targets))
        .init();

    if let Some(err) = parse_err {
        tracing::warn!("invalid log filter {filter:?}: {err}");
    }
    tracing::debug!(?origin, %filter, "logging configured");

    if let Err(err) = engine::run(
        wasm_bindgen_futures::spawn_local,
        engine::EngineConfig::default(),
    ) {
        tracing::error!("engine exited with error: {err}");
    }
}
