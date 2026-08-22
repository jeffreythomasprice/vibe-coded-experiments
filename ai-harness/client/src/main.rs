use leptos::mount::mount_to_body;
use leptos::prelude::*;
use serde::Serialize;
use shared::{GreetRequest, GreetResponse};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], js_name = invoke)]
    async fn invoke(cmd: &str, args: JsValue) -> JsValue;
}

#[derive(Serialize)]
struct GreetArgs {
    request: GreetRequest,
}

fn main() {
    mount_to_body(App);
}

#[component]
fn App() -> impl IntoView {
    let (name, set_name) = signal(String::new());
    let (greeting, set_greeting) = signal(String::new());

    let greet = move |_| {
        let name = name.get();
        wasm_bindgen_futures::spawn_local(async move {
            let request = GreetRequest { name };
            let args = serde_wasm_bindgen::to_value(&GreetArgs { request })
                .expect("failed to serialize GreetRequest");
            let result = invoke("greet", args).await;
            let response: GreetResponse = serde_wasm_bindgen::from_value(result)
                .expect("failed to deserialize GreetResponse");
            set_greeting.set(response.message);
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
