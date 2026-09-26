use std::sync::Mutex;

use tracing_subscriber::prelude::*;

fn main() {
    let loaded = match engine::config::load() {
        Ok(loaded) => loaded,
        Err(err) => {
            eprintln!("config error: {err}");
            std::process::exit(2);
        }
    };

    let (filter, origin) = loaded.config.logging.effective_filter();
    let env_filter = match tracing_subscriber::EnvFilter::try_new(&filter) {
        Ok(env_filter) => env_filter,
        Err(err) => {
            eprintln!("invalid log filter {filter:?}: {err}");
            std::process::exit(2);
        }
    };

    let file_layer = match &loaded.config.logging.file {
        Some(path) => {
            let file = match std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
            {
                Ok(file) => file,
                Err(err) => {
                    eprintln!("failed to open log file {}: {err}", path.display());
                    std::process::exit(2);
                }
            };
            Some(
                tracing_subscriber::fmt::layer()
                    .with_ansi(false)
                    .with_writer(Mutex::new(file)),
            )
        }
        None => None,
    };

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer())
        .with(file_layer)
        .init();

    match &loaded.path {
        Some(path) => tracing::info!(path = %path.display(), ?origin, %filter, "config loaded"),
        None => tracing::info!(?origin, %filter, "no config file found, using defaults"),
    }

    if let Err(err) = engine::run(pollster::block_on, engine::EngineConfig::default()) {
        tracing::error!("engine exited with error: {err}");
        std::process::exit(1);
    }
}
