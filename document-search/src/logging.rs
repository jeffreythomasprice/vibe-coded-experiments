use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use crate::error::Error;

pub fn init() -> Result<(), Error> {
    let crate_target = env!("CARGO_PKG_NAME").replace('-', "_");
    let default_filter = format!("warn,{crate_target}=trace");
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer())
        .try_init()
        .map_err(|e| Error::Logging(e.to_string()))?;
    Ok(())
}
