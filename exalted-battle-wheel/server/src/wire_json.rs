//! A drop-in replacement for `axum::Json<T>` on every route that takes a body: validates the raw
//! request bytes against `T`'s own schema (see `shared::validate`) before ever deserializing them,
//! so a malformed or out-of-bounds body is `ApiError::BadRequest` rather than whatever `serde`
//! happens to do with the wrong shape.

use crate::error::ApiError;
use axum::body::Bytes;
use axum::extract::{FromRequest, Request};
use serde::de::DeserializeOwned;
use shared::validate::{self, WireType};

pub struct WireJson<T>(pub T);

impl<T, S> FromRequest<S> for WireJson<T>
where
    T: DeserializeOwned + WireType,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let bytes = Bytes::from_request(req, state).await.map_err(|error| {
            ApiError::BadRequest(validate::WireError::Syntax(error.to_string()))
        })?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|error| ApiError::BadRequest(validate::WireError::Syntax(error.to_string())))?;
        Ok(WireJson(validate::decode(text)?))
    }
}
