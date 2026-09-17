//! Runtime backstop for what the generated `Deserialize` impls can't express on their own (see
//! `shared/schemas/README.md`'s note on typify's constrained-string support): every message on
//! either side of the wire is validated against the schema that generated its type before it's
//! ever deserialized, not just decoded and hoped-valid. `server`/`client` route their actual
//! decode points through [`decode`]; see each crate's `ws`/`net` module for where.

use jsonschema::Validator;
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::sync::LazyLock;

/// The exact document `shared/build.rs` compiled every generated type from -- merged from
/// `shared/schemas/*.json`, so it can never drift from what's actually generated.
pub const WIRE_SCHEMA: &str = include_str!(concat!(env!("OUT_DIR"), "/wire.schema.json"));

/// One root DTO this module can validate before deserializing. `DEF` is the def name exactly as
/// it appears in `shared/schemas/*.json`'s `$defs`.
pub trait WireType {
    const DEF: &'static str;
}

macro_rules! wire_type {
    ($ty:ty, $def:literal) => {
        impl WireType for $ty {
            const DEF: &'static str = $def;
        }
    };
}

wire_type!(crate::protocol::ClientEnvelope, "ClientEnvelope");
wire_type!(crate::protocol::ServerEnvelope, "ServerEnvelope");
wire_type!(crate::access::CreateAccessCode, "CreateAccessCode");
wire_type!(crate::access::UpdateAccessCode, "UpdateAccessCode");
wire_type!(crate::access::AccessCode, "AccessCode");
wire_type!(crate::access::AccessCodeList, "AccessCodeList");
wire_type!(crate::rooms::RoomList, "RoomList");

#[derive(Debug, thiserror::Error)]
pub enum WireError {
    #[error("invalid JSON: {0}")]
    Syntax(String),
    #[error("at {pointer}: {message}")]
    Invalid { pointer: String, message: String },
}

/// Compiled once per [`WireType`] this module actually validates, not once per def in the merged
/// schema -- most defs exist only to be referenced from one of these, never decoded on their own.
static VALIDATORS: LazyLock<HashMap<&'static str, Validator>> = LazyLock::new(|| {
    let defs: serde_json::Value =
        serde_json::from_str(WIRE_SCHEMA).expect("shared/build.rs always emits a valid JSON document");

    [
        crate::protocol::ClientEnvelope::DEF,
        crate::protocol::ServerEnvelope::DEF,
        crate::access::CreateAccessCode::DEF,
        crate::access::UpdateAccessCode::DEF,
        crate::access::AccessCode::DEF,
        crate::access::AccessCodeList::DEF,
        crate::rooms::RoomList::DEF,
    ]
    .into_iter()
    .map(|def| {
        let schema = serde_json::json!({ "$ref": format!("#/$defs/{def}"), "$defs": defs["$defs"] });
        let validator = jsonschema::validator_for(&schema)
            .unwrap_or_else(|error| panic!("{def} is not a valid schema (checked at build time): {error}"));
        (def, validator)
    })
    .collect()
});

/// Validates `json` against `T::DEF`'s schema, then deserializes it. The one thing this
/// deliberately does *not* do is trust that a payload merely being valid JSON means it's a valid
/// `T` -- a peer with a bug, or one running different code entirely, gets a specific rejection
/// instead of whatever `serde` happens to do with the wrong shape.
pub fn decode<T: DeserializeOwned + WireType>(json: &str) -> Result<T, WireError> {
    let instance: serde_json::Value = serde_json::from_str(json).map_err(|error| WireError::Syntax(error.to_string()))?;

    let validator = VALIDATORS.get(T::DEF).unwrap_or_else(|| panic!("no compiled validator for {} -- add it to shared::validate::VALIDATORS", T::DEF));
    if let Err(error) = validator.validate(&instance) {
        return Err(WireError::Invalid { pointer: error.instance_path().to_string(), message: error.to_string() });
    }

    serde_json::from_value(instance).map_err(|error| WireError::Syntax(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{ClientEnvelope, ClientMessage, RequestId};

    #[test]
    fn accepts_a_well_formed_envelope() {
        let json = r#"{"id":1,"token":"tok","message":{"type":"Leave"}}"#;
        let envelope: ClientEnvelope = decode(json).unwrap();
        assert_eq!(envelope.id, RequestId(1));
        assert!(matches!(envelope.message, ClientMessage::Leave));
    }

    #[test]
    fn rejects_malformed_json() {
        assert!(matches!(decode::<ClientEnvelope>("{not json"), Err(WireError::Syntax(_))));
    }

    #[test]
    fn rejects_an_unknown_message_type() {
        let json = r#"{"id":1,"token":"tok","message":{"type":"NotARealVariant"}}"#;
        assert!(matches!(decode::<ClientEnvelope>(json), Err(WireError::Invalid { .. })));
    }

    #[test]
    fn rejects_a_name_over_the_wire_bound() {
        let long_name = "a".repeat(41);
        let json = format!(r#"{{"id":1,"token":"tok","message":{{"type":"Rename","data":{{"name":"{long_name}"}}}}}}"#);
        assert!(matches!(decode::<ClientEnvelope>(&json), Err(WireError::Invalid { .. })));
    }
}
