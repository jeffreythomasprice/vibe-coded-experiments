//! Shared setup for every DynamoDB-backed store (`access_codes`, `rooms`, `connections`): the
//! HTTPS client construction and RFC3339 timestamp formatting each of their `connect()` functions
//! and item-encoders would otherwise duplicate three times over.

use crate::config::Config;
use aws_config::BehaviorVersion;
use aws_sdk_dynamodb::Client;
use aws_smithy_http_client::tls;
use aws_smithy_http_client::tls::rustls_provider::CryptoMode;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

/// Builds the DynamoDB client from the environment. Infallible: credentials are resolved lazily on
/// the first request, so a missing region or bad key surfaces as a 500 on that request rather than
/// as a startup failure -- which is fine, since `/health` (what the pod's probes hit) never touches
/// DynamoDB.
pub async fn client(config: &Config) -> Client {
    // The SDK's default HTTPS client is rustls over aws-lc-rs, which means compiling several
    // hundred C files inside an emulated-arm64 container on every dependency-layer rebuild. Ring
    // builds from pre-generated asm instead.
    let http_client =
        aws_smithy_http_client::Builder::new().tls_provider(tls::Provider::Rustls(CryptoMode::Ring)).build_https();

    let sdk_config = aws_config::defaults(BehaviorVersion::latest()).http_client(http_client).load().await;

    let mut builder = aws_sdk_dynamodb::config::Builder::from(&sdk_config);
    if let Some(endpoint) = &config.dynamodb_endpoint {
        builder = builder.endpoint_url(endpoint);
    }

    Client::from_conf(builder.build())
}

/// Only fails for years outside 0000-9999, which `OffsetDateTime` cannot hold without the
/// `large-dates` feature, so this never actually happens.
pub fn format_timestamp(at: OffsetDateTime) -> String {
    at.format(&Rfc3339).expect("an in-range timestamp always formats")
}

/// What can go wrong decoding one attribute out of a DynamoDB item, shared by every store's own
/// `item_to_*` function -- `access_codes` only ever produces the first three variants, `rooms`
/// uses all five.
#[derive(Debug, thiserror::Error)]
pub enum ItemError {
    #[error("item has no {0:?} attribute")]
    Missing(&'static str),
    #[error("item attribute {name:?} is not of type {expected}")]
    WrongType { name: &'static str, expected: &'static str },
    #[error("item attribute {name:?} is not an rfc3339 timestamp {value:?}: {source}")]
    Timestamp { name: &'static str, value: String, source: time::error::Parse },
    #[error("item attribute {name:?} is not a valid unix timestamp {value}: {source}")]
    Epoch { name: &'static str, value: i64, source: time::error::ComponentRange },
    #[error("item attribute {name:?} is not a valid number {value:?}: {source}")]
    Number { name: &'static str, value: String, source: std::num::ParseIntError },
    #[error("item attribute {name:?} is not valid json: {source}")]
    Json { name: &'static str, source: serde_json::Error },
}
