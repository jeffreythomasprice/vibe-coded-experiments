use gloo_net::http::RequestBuilder;

fn with_auth(req: RequestBuilder) -> RequestBuilder {
    if let Some(token) = crate::auth::get_token() {
        req.header("Authorization", &format!("Bearer {token}"))
    } else {
        req
    }
}

pub fn get(url: &str) -> RequestBuilder {
    with_auth(gloo_net::http::Request::get(url))
}

pub fn post(url: &str) -> RequestBuilder {
    with_auth(
        gloo_net::http::Request::post(url).header("Content-Type", "application/json"),
    )
}

pub fn put(url: &str) -> RequestBuilder {
    with_auth(
        gloo_net::http::Request::put(url).header("Content-Type", "application/json"),
    )
}

pub fn delete(url: &str) -> RequestBuilder {
    with_auth(gloo_net::http::Request::delete(url))
}
