fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    if let Err(err) = engine::run(pollster::block_on, engine::EngineConfig::default()) {
        tracing::error!("engine exited with error: {err}");
        std::process::exit(1);
    }
}
