use crate::protocol::{Request, Response};
use crate::server::ServerState;

pub async fn dispatch(req: Request, _state: &ServerState) -> Response {
    match req {
        Request::Ping => handle_ping().await,
    }
}

async fn handle_ping() -> Response {
    Response::Pong
}
