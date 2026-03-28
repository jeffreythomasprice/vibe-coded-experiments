use leptos::prelude::*;
use web_sys::window;

const TOKEN_KEY: &str = "auth_token";

#[derive(Clone, Copy)]
pub struct AuthState(pub RwSignal<bool>);

impl AuthState {
    pub fn new() -> Self {
        Self(RwSignal::new(is_authenticated()))
    }

    pub fn set_authenticated(&self, val: bool) {
        self.0.set(val);
    }
}

pub fn get_token() -> Option<String> {
    let storage = window()?.local_storage().ok()??;
    storage.get_item(TOKEN_KEY).ok()?
}

pub fn set_token(token: &str) {
    if let Some(storage) = window()
        .and_then(|w| w.local_storage().ok())
        .flatten()
    {
        let _ = storage.set_item(TOKEN_KEY, token);
    }
}

pub fn remove_token() {
    if let Some(storage) = window()
        .and_then(|w| w.local_storage().ok())
        .flatten()
    {
        let _ = storage.remove_item(TOKEN_KEY);
    }
}

pub fn is_authenticated() -> bool {
    let token = match get_token() {
        Some(t) => t,
        None => return false,
    };
    match decode_jwt_exp(&token) {
        Some(exp) => {
            let now_secs = js_sys::Date::now() / 1000.0;
            exp > now_secs
        }
        None => false,
    }
}

#[derive(serde::Deserialize)]
struct JwtPayload {
    exp: f64,
}

fn decode_jwt_exp(token: &str) -> Option<f64> {
    let payload_b64url = token.split('.').nth(1)?;
    // base64url -> standard base64
    let b64: String = payload_b64url
        .chars()
        .map(|c| match c {
            '-' => '+',
            '_' => '/',
            other => other,
        })
        .collect();
    let padded = match b64.len() % 4 {
        2 => format!("{b64}=="),
        3 => format!("{b64}="),
        _ => b64,
    };
    let decoded = window()?.atob(&padded).ok()?;
    let payload: JwtPayload = serde_json::from_str(&decoded).ok()?;
    Some(payload.exp)
}
