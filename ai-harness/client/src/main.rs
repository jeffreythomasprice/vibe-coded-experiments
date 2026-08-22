mod app;
mod commands;
mod ipc;
mod logging;
mod markdown;
mod models;
mod sidebar;
mod transcript;
mod views;

use leptos::mount::mount_to_body;

fn main() {
    logging::init();
    tracing::info!("client starting");
    mount_to_body(app::App);
}
