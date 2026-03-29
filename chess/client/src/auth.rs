use leptos::prelude::*;
use web_sys::window;

const TOKEN_KEY: &str = "auth_token";

#[derive(Clone, Copy)]
pub struct AuthState {
    pub authenticated: RwSignal<bool>,
    pub is_admin: RwSignal<bool>,
}

impl AuthState {
    pub fn new() -> Self {
        let claims = get_token().and_then(|t| decode_jwt_claims(&t));
        let authenticated = claims.as_ref().map_or(false, |c| {
            let now_secs = js_sys::Date::now() / 1000.0;
            c.exp > now_secs
        });
        let is_admin = claims.map_or(false, |c| c.is_admin);
        Self {
            authenticated: RwSignal::new(authenticated),
            is_admin: RwSignal::new(is_admin),
        }
    }

    pub fn set_authenticated(&self, val: bool) {
        self.authenticated.set(val);
        if val {
            let claims = get_token().and_then(|t| decode_jwt_claims(&t));
            self.is_admin.set(claims.map_or(false, |c| c.is_admin));
        } else {
            self.is_admin.set(false);
        }
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
    match decode_jwt_claims(&token) {
        Some(claims) => {
            let now_secs = js_sys::Date::now() / 1000.0;
            claims.exp > now_secs
        }
        None => false,
    }
}

#[derive(serde::Deserialize)]
struct JwtClaims {
    exp: f64,
    #[serde(default)]
    is_admin: bool,
}

fn decode_jwt_claims(token: &str) -> Option<JwtClaims> {
    let payload_b64url = token.split('.').nth(1)?;
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
    serde_json::from_str(&decoded).ok()
}
