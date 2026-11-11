mod hub;
mod shot;

use editor_cmd::Response;

pub use editor_proto::Policy;
pub use hub::{ConnId, Handled, Hub};

pub struct HostReply {
    pub response: Response,
    pub png: Option<Vec<u8>>,
}
