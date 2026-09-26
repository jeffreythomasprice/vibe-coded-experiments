use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Gpu(#[from] crate::render::gpu::GpuError),
    #[error("event loop error: {0}")]
    EventLoop(#[from] winit::error::EventLoopError),
    #[error("window creation failed: {0}")]
    Window(#[from] winit::error::OsError),
    #[error(transparent)]
    Text(#[from] crate::text::TextError),
}
