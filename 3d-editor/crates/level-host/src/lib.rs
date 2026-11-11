mod hub;
mod shot;

use level_cmd::Response;

pub use hub::{ConnId, Handled, Hub};
pub use level_proto::Policy;

pub struct HostReply {
    pub response: Response,
    pub png: Option<Vec<u8>>,
}
