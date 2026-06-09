mod app;
mod logging;
mod ws;

use app::App;

fn main() {
    console_error_panic_hook::set_once();
    logging::init();
    tracing::trace!("client starting");
    leptos::mount::mount_to_body(App);
}
