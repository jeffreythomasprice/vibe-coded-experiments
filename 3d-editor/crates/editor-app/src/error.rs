#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("event loop: {0}")]
    EventLoop(#[from] winit::error::EventLoopError),
    #[error("window: {0}")]
    Window(#[from] winit::error::OsError),
    #[error("surface: {0}")]
    Surface(#[from] wgpu::CreateSurfaceError),
    #[error("no graphics adapter: {0}")]
    Adapter(#[from] wgpu::RequestAdapterError),
    #[error("graphics device: {0}")]
    Device(#[from] wgpu::RequestDeviceError),
    #[error("the surface supports no usable configuration")]
    SurfaceConfig,
    #[error("level-server: {0}")]
    Server(#[from] crate::remote::RemoteError),
}
