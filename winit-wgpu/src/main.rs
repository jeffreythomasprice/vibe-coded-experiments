use anyhow::Result;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new(format!(
                    "info,wgpu_core::device::resource=warn,{}=trace",
                    env!("CARGO_CRATE_NAME")
                ))
            }),
        )
        .init();
    winit_wgpu::run()
}

#[cfg(target_arch = "wasm32")]
fn main() {}
