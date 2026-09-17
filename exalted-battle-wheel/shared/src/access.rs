//! Wire types for the server's access-code HTTP API — what a client sends and receives to check
//! or manage an access code. Unlike `protocol`, which crosses a websocket, this crosses a plain
//! HTTP connection between the client and `server`. Defined in `shared/schemas/access.json`; see
//! `shared/schemas/README.md`.

pub use crate::generated::{AccessCode, AccessCodeList, AccessKey, ApiErrorBody, CreateAccessCode, UpdateAccessCode};
