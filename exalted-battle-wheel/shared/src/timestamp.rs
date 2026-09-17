//! The `Timestamp` def in `shared/schemas/common.json` is replaced by this hand-written type
//! rather than generated (see `shared/schemas/README.md`), because generated code has no way to
//! attach `#[serde(with = "time::serde::rfc3339")]` to a field. This wraps that exact encoding so
//! the wire bytes are unaffected by the swap.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::ops::Deref;
use time::OffsetDateTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(pub OffsetDateTime);

impl From<OffsetDateTime> for Timestamp {
    fn from(value: OffsetDateTime) -> Self {
        Self(value)
    }
}

impl Deref for Timestamp {
    type Target = OffsetDateTime;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Serialize for Timestamp {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        time::serde::rfc3339::serialize(&self.0, serializer)
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        time::serde::rfc3339::deserialize(deserializer).map(Timestamp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::{Date, Month};

    #[test]
    fn round_trips_as_a_bare_rfc3339_string_not_a_wrapped_object() {
        let date = Date::from_calendar_date(2024, Month::January, 15).unwrap();
        let timestamp = Timestamp(date.with_hms(9, 30, 0).unwrap().assume_utc());
        let json = serde_json::to_string(&timestamp).unwrap();
        assert_eq!(json, "\"2024-01-15T09:30:00Z\"");
        assert_eq!(serde_json::from_str::<Timestamp>(&json).unwrap(), timestamp);
    }
}
