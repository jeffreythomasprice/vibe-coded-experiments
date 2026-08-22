mod ipc;
mod logging;

use ipc::invoke;
use leptos::mount::mount_to_body;
use leptos::prelude::*;
use serde::Serialize;
use shared::{GreetRequest, GreetResponse};
use wasm_bindgen::JsValue;

#[derive(Serialize)]
struct GreetArgs {
    request: GreetRequest,
}

fn main() {
    logging::init();
    tracing::info!("client starting");
    mount_to_body(App);
}

#[component]
fn App() -> impl IntoView {
    let (name, set_name) = signal(String::new());
    let (greeting, set_greeting) = signal(String::new());

    let greet = move |_| {
        let name = name.get();
        wasm_bindgen_futures::spawn_local(async move {
            tracing::debug!(%name, "invoking greet");
            let request = GreetRequest { name };
            let args = match serde_wasm_bindgen::to_value(&GreetArgs { request }) {
                Ok(args) => args,
                Err(err) => {
                    tracing::error!(%err, "failed to serialize GreetRequest");
                    return;
                }
            };
            let result: JsValue = invoke("greet", args).await;
            match serde_wasm_bindgen::from_value::<GreetResponse>(result) {
                Ok(response) => {
                    tracing::trace!(message = %response.message, "greet returned");
                    set_greeting.set(response.message);
                }
                Err(err) => tracing::error!(%err, "failed to deserialize GreetResponse"),
            }
        });
    };

    view! {
        <main>
            <h1>"ai-harness"</h1>
            <p>"Hello world! Type a name and click the button to call the Rust backend."</p>
            <input
                type="text"
                placeholder="Enter a name..."
                on:input:target=move |ev| set_name.set(ev.target().value())
            />
            <button on:click=greet>"Greet"</button>
            <p>{move || greeting.get()}</p>
        </main>
    }
}
