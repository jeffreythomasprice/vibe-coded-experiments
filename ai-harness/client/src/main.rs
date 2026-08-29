mod app;
mod commands;
mod composer;
mod confirm;
mod icons;
mod ipc;
mod logging;
mod markdown;
mod models;
mod runs;
mod sidebar;
mod spinner;
mod theme;
mod transcript;
mod views;

use leptos::mount::mount_to_body;

fn main() {
    logging::init();
    tracing::info!("client starting");
    mount_to_body(app::App);
}
