use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{FaceId, IdParseError, ObjectId, VertexId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Part {
    /// The whole object: every face (or every vertex, for vertex operations).
    All,
    Face(FaceId),
    Vertex(VertexId),
}

/// `b12` (whole object), `b12/f3` (face) or `m3/v10` (mesh vertex).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ElementRef {
    pub object: ObjectId,
    pub part: Part,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ElementParseError {
    #[error(transparent)]
    Id(#[from] IdParseError),
    #[error("expected `f<number>` or `v<number>` after the `/`")]
    BadPart,
}

impl fmt::Display for ElementRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.part {
            Part::All => write!(f, "{}", self.object),
            Part::Face(id) => write!(f, "{}/f{}", self.object, id.0),
            Part::Vertex(id) => write!(f, "{}/v{}", self.object, id.0),
        }
    }
}

impl FromStr for ElementRef {
    type Err = ElementParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let Some((object, part)) = s.split_once('/') else {
            return Ok(Self {
                object: s.parse()?,
                part: Part::All,
            });
        };
        let object = object.parse()?;
        let number = |rest: &str| rest.parse::<u32>().map_err(|_| ElementParseError::BadPart);
        let part = match part.chars().next() {
            Some('f') => Part::Face(FaceId(number(&part[1..])?)),
            Some('v') => Part::Vertex(VertexId(number(&part[1..])?)),
            _ => return Err(ElementParseError::BadPart),
        };
        Ok(Self { object, part })
    }
}

impl Serialize for ElementRef {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for ElementRef {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

impl schemars::JsonSchema for ElementRef {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ElementRef".into()
    }

    fn inline_schema() -> bool {
        true
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "pattern": "^[bmegilc][0-9]+(/[fv][0-9]+)?$",
            "description": "An object (`b12`, all its faces), a face (`b12/f3`, `m3/f2`) or a mesh vertex (`m3/v10`)",
            "examples": ["b12/f3"],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BrushId;

    #[test]
    fn round_trip() {
        for s in ["b12", "b12/f3", "m3/v10"] {
            assert_eq!(s.parse::<ElementRef>().unwrap().to_string(), s);
        }
        let r: ElementRef = "b4/f2".parse().unwrap();
        assert_eq!(r.object, ObjectId::Brush(BrushId(4)));
        assert_eq!(r.part, Part::Face(FaceId(2)));
    }

    #[test]
    fn errors() {
        assert_eq!(
            "b1/x3".parse::<ElementRef>(),
            Err(ElementParseError::BadPart)
        );
        assert_eq!(
            "b1/f".parse::<ElementRef>(),
            Err(ElementParseError::BadPart)
        );
        assert!("z1".parse::<ElementRef>().is_err());
        assert!("".parse::<ElementRef>().is_err());
    }
}
