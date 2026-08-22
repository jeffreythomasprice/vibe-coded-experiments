mod catalog;
mod ipc;
mod logging;

use leptos::mount::mount_to_body;

fn main() {
    logging::init();
    tracing::info!("client starting");
    mount_to_body(catalog::App);
}
