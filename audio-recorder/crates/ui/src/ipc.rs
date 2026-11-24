use audio_recorder_schema::{Schema, SchemaError, from_value, ipc::IpcError, to_value};
use serde::Serialize;
use wasm_bindgen::{JsValue, prelude::*};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], catch)]
    async fn invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"], catch)]
    async fn listen(event: &str, handler: &Closure<dyn FnMut(JsValue)>) -> Result<JsValue, JsValue>;
}

#[derive(Debug, thiserror::Error)]
pub enum IpcFailure {
    #[error(transparent)]
    Schema(#[from] SchemaError),
    #[error("{0}")]
    Remote(String),
    #[error("transport error: {0}")]
    Transport(String),
}

fn to_js(value: &serde_json::Value) -> Result<JsValue, IpcFailure> {
    value
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(|e| IpcFailure::Transport(e.to_string()))
}

fn from_js(value: JsValue) -> Result<serde_json::Value, IpcFailure> {
    serde_wasm_bindgen::from_value(value).map_err(|e| IpcFailure::Transport(e.to_string()))
}

pub async fn call<A: Schema, R: Schema>(command: &str, args: &A) -> Result<R, IpcFailure> {
    let payload = serde_json::json!({ "args": to_value(args)? });
    match invoke(command, to_js(&payload)?).await {
        Ok(result) => Ok(from_value(from_js(result)?)?),
        Err(err) => {
            let raw = from_js(err).unwrap_or(serde_json::Value::Null);
            match from_value::<IpcError>(raw.clone()) {
                Ok(e) => Err(IpcFailure::Remote(e.message)),
                Err(_) => Err(IpcFailure::Remote(raw.to_string())),
            }
        }
    }
}

pub fn listen_event<T: Schema + 'static>(event: &'static str, mut handler: impl FnMut(T) + 'static) {
    wasm_bindgen_futures::spawn_local(async move {
        let closure = Closure::<dyn FnMut(JsValue)>::new(move |envelope: JsValue| {
            let payload = js_sys::Reflect::get(&envelope, &JsValue::from_str("payload")).unwrap_or(JsValue::NULL);
            match from_js(payload).and_then(|v| Ok(from_value::<T>(v)?)) {
                Ok(value) => handler(value),
                Err(e) => web_error(&format!("invalid {event} payload: {e}")),
            }
        });
        if let Err(e) = listen(event, &closure).await {
            web_error(&format!("failed to listen to {event}: {e:?}"));
        }
        closure.forget();
    });
}

pub fn web_error(message: &str) {
    web_sys::console::error_1(&JsValue::from_str(message));
}
