//! The client is compiled to wasm, so it has no access to environment variables
//! at runtime. Instead we read `client/.env` here at build time and bake the
//! values in as compile-time env vars consumed via `env!(...)` in the app.
//!
//! Committed defaults point at the local dev server (localhost:8001). Override
//! by editing `.env` (or setting the vars in the build environment).

use std::collections::HashMap;
use std::path::Path;

fn main() {
    // Committed defaults.
    let mut vars: HashMap<String, String> = HashMap::new();
    vars.insert(
        "SERVER_HTTP_URL".to_string(),
        "http://localhost:8001".to_string(),
    );
    vars.insert(
        "SERVER_WS_URL".to_string(),
        "ws://localhost:8001/ws".to_string(),
    );

    // Overlay anything found in client/.env.
    let env_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(".env");
    println!("cargo:rerun-if-changed=.env");
    if let Ok(iter) = dotenvy::from_path_iter(&env_path) {
        for (key, value) in iter.flatten() {
            vars.insert(key, value);
        }
    }

    for (key, value) in &vars {
        println!("cargo:rustc-env={key}={value}");
    }
}
