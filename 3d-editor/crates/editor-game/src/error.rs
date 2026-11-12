#[derive(Debug, thiserror::Error)]
pub enum GameError {
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
    #[error(transparent)]
    Config(#[from] editor_proto::ConfigError),
    #[error("editor-server: {0}")]
    Client(#[from] editor_proto::ClientError),
    #[error("editor-server refused the request: {0}")]
    Refused(String),
    #[error(transparent)]
    Replica(#[from] editor_proto::ReplicaError),
    #[error("usage: editor-game [scene.toml | --session <id>]")]
    Usage,
}
