mod app;
mod components;
mod fmt;
mod ipc;
mod markdown;
mod store;
mod theme;

fn main() {
    console_error_panic_hook::set_once();
    theme::install(&theme::all());
    leptos::mount::mount_to_body(app::App);
}
