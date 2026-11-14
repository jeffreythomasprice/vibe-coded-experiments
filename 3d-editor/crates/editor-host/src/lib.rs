mod hub;
pub mod recovery;
mod shot;

use editor_cmd::Response;

pub use editor_proto::Policy;
pub use hub::{ConnId, Handled, Hub};
pub use recovery::{Job, RecoveryError, RecoverySettings};

pub struct HostReply {
    pub response: Response,
    pub png: Option<Vec<u8>>,
}
