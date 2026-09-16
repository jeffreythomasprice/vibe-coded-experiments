//! Signs and verifies the room session token handed to every member on `Joined`, so a browser
//! that closes and reopens can prove which room (and, if applicable, host status) it belonged to
//! without the server keeping any state about it beyond the room itself. HS256, keyed by
//! `SESSION_SECRET` (see `config.rs`) — a Terraform-managed secret so it survives a redeploy; see
//! `CLAUDE.md`'s "Server hosting" section.

use crate::rooms::RoomId;
use jsonwebtoken::errors::ErrorKind;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use shared::protocol::SessionRejection;
use std::sync::Arc;
use time::OffsetDateTime;

/// How long a room session stays valid after it's issued — long enough that a browser closed
/// overnight still comes back, short enough that a token leaked once doesn't matter forever.
const SESSION_TTL: time::Duration = time::Duration::hours(12);

#[derive(Serialize, Deserialize)]
struct Claims {
    /// The room's id, never its name -- a name freed by expiry and taken by a different room must
    /// never accept a session minted for the old one. See `SessionRejection::WrongRoom`.
    rid: String,
    host: bool,
    exp: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Session {
    pub host: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("could not sign a room session")]
    Sign(#[source] jsonwebtoken::errors::Error),
}

#[derive(Clone)]
pub struct Sessions {
    encoding: Arc<EncodingKey>,
    decoding: Arc<DecodingKey>,
}

impl Sessions {
    pub fn new(secret: &str) -> Self {
        Self { encoding: Arc::new(EncodingKey::from_secret(secret.as_bytes())), decoding: Arc::new(DecodingKey::from_secret(secret.as_bytes())) }
    }

    pub fn issue(&self, room: &RoomId, host: bool) -> Result<String, SessionError> {
        let exp = (OffsetDateTime::now_utc() + SESSION_TTL).unix_timestamp();
        let claims = Claims { rid: room.0.clone(), host, exp };
        encode(&Header::new(Algorithm::HS256), &claims, &self.encoding).map_err(SessionError::Sign)
    }

    pub fn verify(&self, token: &str, room: &RoomId) -> Result<Session, SessionRejection> {
        let validation = Validation::new(Algorithm::HS256);
        let claims = decode::<Claims>(token, &self.decoding, &validation)
            .map_err(|error| match error.kind() {
                ErrorKind::ExpiredSignature => SessionRejection::Expired,
                _ => SessionRejection::Malformed,
            })?
            .claims;
        if claims.rid != room.0 {
            return Err(SessionRejection::WrongRoom);
        }
        Ok(Session { host: claims.host })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn room(id: &str) -> RoomId {
        RoomId(id.to_string())
    }

    #[test]
    fn a_freshly_issued_token_verifies_for_its_own_room() {
        let sessions = Sessions::new("test-secret");
        let token = sessions.issue(&room("room-1"), true).unwrap();
        let session = sessions.verify(&token, &room("room-1")).unwrap();
        assert!(session.host);
    }

    #[test]
    fn a_token_is_refused_against_a_different_room() {
        let sessions = Sessions::new("test-secret");
        let token = sessions.issue(&room("room-1"), false).unwrap();
        assert_eq!(sessions.verify(&token, &room("room-2")), Err(SessionRejection::WrongRoom));
    }

    #[test]
    fn a_tampered_token_is_malformed() {
        let sessions = Sessions::new("test-secret");
        let other = Sessions::new("a-different-secret");
        let token = other.issue(&room("room-1"), false).unwrap();
        assert_eq!(sessions.verify(&token, &room("room-1")), Err(SessionRejection::Malformed));
    }

    #[test]
    fn an_expired_token_is_reported_as_expired() {
        let sessions = Sessions::new("test-secret");
        let claims = Claims { rid: "room-1".to_string(), host: false, exp: (OffsetDateTime::now_utc() - time::Duration::hours(1)).unix_timestamp() };
        let token = encode(&Header::new(Algorithm::HS256), &claims, &sessions.encoding).unwrap();
        assert_eq!(sessions.verify(&token, &room("room-1")), Err(SessionRejection::Expired));
    }
}
