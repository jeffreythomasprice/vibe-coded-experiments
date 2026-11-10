use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IdParseError {
    #[error("empty id")]
    Empty,
    #[error("expected prefix '{expected}'")]
    WrongPrefix { expected: char },
    #[error("invalid number in id")]
    InvalidNumber,
    #[error("unknown id prefix")]
    UnknownPrefix,
}

macro_rules! global_id {
    ($name:ident, $prefix:literal) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub u64);

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}{}", $prefix, self.0)
            }
        }

        impl FromStr for $name {
            type Err = IdParseError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let mut chars = s.chars();
                match chars.next() {
                    None => Err(IdParseError::Empty),
                    Some($prefix) => chars
                        .as_str()
                        .parse::<u64>()
                        .map($name)
                        .map_err(|_| IdParseError::InvalidNumber),
                    Some(_) => Err(IdParseError::WrongPrefix { expected: $prefix }),
                }
            }
        }

        impl schemars::JsonSchema for $name {
            fn schema_name() -> std::borrow::Cow<'static, str> {
                stringify!($name).into()
            }

            fn inline_schema() -> bool {
                true
            }

            fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
                schemars::json_schema!({
                    "type": "string",
                    "pattern": concat!("^", $prefix, "[0-9]+$"),
                    "examples": [concat!($prefix, "12")],
                })
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.collect_str(self)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let s = String::deserialize(d)?;
                s.parse().map_err(serde::de::Error::custom)
            }
        }
    };
}

global_id!(BrushId, 'b');
global_id!(MeshId, 'm');
global_id!(EntityId, 'e');
global_id!(GroupId, 'g');
global_id!(LayerId, 'l');
global_id!(CameraId, 'c');

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct FaceId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct VertexId(pub u32);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        assert_eq!(BrushId(12).to_string(), "b12");
        assert_eq!("m3".parse::<MeshId>(), Ok(MeshId(3)));
    }

    #[test]
    fn errors() {
        assert_eq!("".parse::<BrushId>(), Err(IdParseError::Empty));
        assert_eq!(
            "m3".parse::<BrushId>(),
            Err(IdParseError::WrongPrefix { expected: 'b' })
        );
        assert_eq!("bx".parse::<BrushId>(), Err(IdParseError::InvalidNumber));
    }
}
