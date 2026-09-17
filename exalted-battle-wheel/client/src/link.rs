//! Query-string parameters an invite link carries, and the link itself. Reading and building both
//! live here rather than in `api/mod.rs` (which owns percent-encoding for a *path* segment) or
//! `ui/room.rs` (which only needs the finished string) -- URL grammar is its own concern.

use leptos::web_sys;

pub const AUTH_CODE_PARAM: &str = "auth_code";
pub const JOIN_ROOM_PARAM: &str = "join_room";

/// The two query parameters an invite link carries, as read off the address bar at startup.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct StartupParams {
    pub auth_code: Option<String>,
    pub join_room: Option<String>,
}

/// Percent-encodes a single query-string value (RFC 3986's unreserved set survives unescaped).
/// Hand-rolled rather than a JS-side encoder so it's plain, host-testable logic -- this crate has
/// no wasm-bindgen-test setup. The same rule serves a path segment (see `api::mod`'s caller) and a
/// query value; nothing about either grammar calls for a different reserved set here.
pub fn percent_encode_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => encoded.push(byte as char),
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

/// The link a host hands out: this page's own origin and path (never whatever else happens to be
/// in the host's own address bar at the moment) plus the two startup parameters.
pub fn invite_url(origin: &str, path: &str, auth_code: &str, room: &str) -> String {
    let origin = origin.trim_end_matches('/');
    let path = if path.starts_with('/') { path } else { "/" };
    format!(
        "{origin}{path}?{AUTH_CODE_PARAM}={}&{JOIN_ROOM_PARAM}={}",
        percent_encode_component(auth_code),
        percent_encode_component(room),
    )
}

/// The address bar's new value once the startup parameters are out of it -- `?`/`#` are dropped
/// entirely when their part is empty, rather than left dangling.
pub fn scrubbed_url(path: &str, remaining_query: &str, fragment: &str) -> String {
    let mut url = path.to_string();
    if !remaining_query.is_empty() {
        url.push('?');
        url.push_str(remaining_query);
    }
    if !fragment.is_empty() {
        url.push('#');
        url.push_str(fragment);
    }
    url
}

/// This page's own origin and path, for `invite_url`. `None` only if `window()`/`location()` are
/// unavailable, which the invite button treats as "nothing to build a link from."
pub fn origin_and_path() -> Option<(String, String)> {
    let location = web_sys::window()?.location();
    Some((location.origin().ok()?, location.pathname().ok()?))
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty()).map(|value| value.trim().to_string())
}

/// Reads both startup parameters and rewrites the address bar without them, in one synchronous
/// step, so a page reload can never replay an invite this load already consumed. Silent on
/// failure to tidy the URL -- the values are already in Rust by then, and a link that merely fails
/// to scrub itself isn't worth a toast.
pub fn take_startup_params() -> StartupParams {
    let Some(window) = web_sys::window() else { return StartupParams::default() };
    let location = window.location();
    let Ok(search) = location.search() else { return StartupParams::default() };
    let Ok(params) = web_sys::UrlSearchParams::new_with_str(&search) else { return StartupParams::default() };

    let auth_code = non_empty(params.get(AUTH_CODE_PARAM));
    let join_room = non_empty(params.get(JOIN_ROOM_PARAM));
    if auth_code.is_none() && join_room.is_none() {
        return StartupParams { auth_code, join_room };
    }

    params.delete(AUTH_CODE_PARAM);
    params.delete(JOIN_ROOM_PARAM);
    // `UrlSearchParams` has no `to_string` of its own in web-sys; this resolves through
    // `Deref<Target = js_sys::Object>` to `Object::to_string`, which is bound to `js_name =
    // toString` and so actually calls `URLSearchParams.prototype.toString` (the serialized
    // query), not `Object.prototype.toString`. Reads like a bug; isn't one.
    let remaining_query = String::from(params.to_string());
    let path = location.pathname().unwrap_or_default();
    let fragment = location.hash().unwrap_or_default().trim_start_matches('#').to_string();
    let clean = scrubbed_url(&path, &remaining_query, &fragment);

    if let Ok(history) = window.history()
        && let Err(error) = history.replace_state_with_url(&leptos::wasm_bindgen::JsValue::NULL, "", Some(&clean))
    {
        tracing::warn!(?error, "could not scrub startup parameters from the address bar");
    }

    StartupParams { auth_code, join_room }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_encoding_leaves_unreserved_characters_alone() {
        assert_eq!(percent_encode_component("player-one.2_3~"), "player-one.2_3~");
    }

    #[test]
    fn percent_encoding_escapes_everything_else() {
        assert_eq!(percent_encode_component("a/b c#d"), "a%2Fb%20c%23d");
    }

    #[test]
    fn percent_encoding_escapes_a_literal_plus_so_it_cant_be_read_back_as_a_space() {
        assert_eq!(percent_encode_component("a+b"), "a%2Bb");
    }

    #[test]
    fn percent_encoding_escapes_non_ascii_per_utf8_byte() {
        assert_eq!(percent_encode_component("é"), "%C3%A9");
    }

    #[test]
    fn invite_url_carries_both_encoded_parameters() {
        let url = invite_url("https://exalted.jeffrey.lol", "/", "hunter 2", "tuesday's game");
        assert_eq!(url, "https://exalted.jeffrey.lol/?auth_code=hunter%202&join_room=tuesday%27s%20game");
    }

    #[test]
    fn invite_url_trims_a_trailing_slash_off_the_origin() {
        let url = invite_url("https://exalted.jeffrey.lol/", "/", "a", "b");
        assert_eq!(url, "https://exalted.jeffrey.lol/?auth_code=a&join_room=b");
    }

    #[test]
    fn invite_url_falls_back_to_root_for_an_empty_path() {
        let url = invite_url("https://exalted.jeffrey.lol", "", "a", "b");
        assert_eq!(url, "https://exalted.jeffrey.lol/?auth_code=a&join_room=b");
    }

    #[test]
    fn scrubbed_url_drops_the_query_and_fragment_markers_when_both_are_empty() {
        assert_eq!(scrubbed_url("/", "", ""), "/");
    }

    #[test]
    fn scrubbed_url_keeps_an_unrelated_parameter_and_the_fragment() {
        assert_eq!(scrubbed_url("/", "theme=dark", "log"), "/?theme=dark#log");
    }
}
