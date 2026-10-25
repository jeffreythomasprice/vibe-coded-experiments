use wasm_bindgen::JsValue;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("no global window")]
    NoWindow,
    #[error("window has no document")]
    NoDocument,
    #[error("element #{0} not found")]
    ElementNotFound(&'static str),
    #[error("element #{0} is not a canvas")]
    NotACanvas(&'static str),
    #[error("javascript error: {0}")]
    Js(String),
    #[error("invalid font: {0}")]
    Font(#[from] ab_glyph::InvalidFont),
    #[cfg(target_arch = "wasm32")]
    #[error("event loop error: {0}")]
    EventLoop(#[from] winit::error::EventLoopError),
    #[cfg(target_arch = "wasm32")]
    #[error("window error: {0}")]
    Window(#[from] winit::error::OsError),
    #[cfg(target_arch = "wasm32")]
    #[error("could not create surface: {0}")]
    Surface(#[from] wgpu::CreateSurfaceError),
    #[cfg(target_arch = "wasm32")]
    #[error("no suitable gpu adapter: {0}")]
    Adapter(#[from] wgpu::RequestAdapterError),
    #[cfg(target_arch = "wasm32")]
    #[error("could not create gpu device: {0}")]
    Device(#[from] wgpu::RequestDeviceError),
    #[cfg(target_arch = "wasm32")]
    #[error("surface is not supported by the adapter")]
    SurfaceConfig,
}

impl From<JsValue> for AppError {
    fn from(value: JsValue) -> Self {
        AppError::Js(format!("{value:?}"))
    }
}

impl From<AppError> for JsValue {
    fn from(value: AppError) -> Self {
        JsValue::from_str(&value.to_string())
    }
}
