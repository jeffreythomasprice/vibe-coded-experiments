//! Build-time config baked into the wasm bundle by `build.rs`.

pub const API_BASE_URL: &str = env!("API_BASE_URL");
