use crate::protocol::{Request, Response};
use crate::server::ServerState;

pub async fn dispatch(req: Request, _state: &ServerState) -> Response {
    match req {
        Request::Ping => handle_ping().await,
        Request::Chat { message } => handle_chat(message).await,
    }
}

async fn handle_ping() -> Response {
    Response::Pong
}

async fn handle_chat(message: String) -> Response {
    Response::Chat {
        reply: format!("(placeholder reply to: {message})"),
    }
}
