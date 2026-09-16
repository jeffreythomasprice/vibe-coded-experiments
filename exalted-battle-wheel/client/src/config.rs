//! Build-time config baked into the wasm bundle by `build.rs`.

pub const API_BASE_URL: &str = env!("API_BASE_URL");

/// The server's `/ws` endpoint, derived from `API_BASE_URL` rather than a second baked-in
/// constant — the two always name the same host, and a websocket URL is just the http(s) one with
/// its scheme swapped for ws(s).
pub fn ws_url() -> String {
    ws_url_from(API_BASE_URL)
}

fn ws_url_from(api_base_url: &str) -> String {
    let base = api_base_url.trim_end_matches('/');
    let base = match base.strip_prefix("https://") {
        Some(rest) => format!("wss://{rest}"),
        None => match base.strip_prefix("http://") {
            Some(rest) => format!("ws://{rest}"),
            None => base.to_string(),
        },
    };
    format!("{base}/ws")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upgrades_https_to_wss() {
        assert_eq!(ws_url_from("https://exalted-api.jeffrey.lol"), "wss://exalted-api.jeffrey.lol/ws");
    }

    #[test]
    fn upgrades_http_to_ws() {
        assert_eq!(ws_url_from("http://127.0.0.1:8001"), "ws://127.0.0.1:8001/ws");
    }

    #[test]
    fn strips_a_trailing_slash_before_appending_the_path() {
        assert_eq!(ws_url_from("http://127.0.0.1:8001/"), "ws://127.0.0.1:8001/ws");
    }
}
