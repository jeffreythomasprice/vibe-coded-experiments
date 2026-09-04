mod app;
mod ui;

use app::App;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use tracing_web::MakeWebConsoleWriter;

#[cfg(debug_assertions)]
const OWN_CRATE_LOG_LEVEL: &str = "trace";
#[cfg(not(debug_assertions))]
const OWN_CRATE_LOG_LEVEL: &str = "error";

fn init_logging() {
    console_error_panic_hook::set_once();

    let filter = EnvFilter::new(format!("error,exalted_battle_wheel={OWN_CRATE_LOG_LEVEL}"));

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .without_time()
        .with_writer(MakeWebConsoleWriter::new());

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .init();
}

fn main() {
    init_logging();
    tracing::info!("starting exalted-battle-wheel");

    leptos::mount::mount_to_body(App);
}
