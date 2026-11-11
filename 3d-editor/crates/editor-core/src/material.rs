use std::fmt;

use base64::Engine;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{Extra, MaterialId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ImageFormat {
    Png,
    Jpeg,
    Tga,
    Bmp,
}

impl ImageFormat {
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "png" => Some(Self::Png),
            "jpg" | "jpeg" => Some(Self::Jpeg),
            "tga" => Some(Self::Tga),
            "bmp" => Some(Self::Bmp),
            _ => None,
        }
    }
}

impl fmt::Display for ImageFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Png => "png",
            Self::Jpeg => "jpeg",
            Self::Tga => "tga",
            Self::Bmp => "bmp",
        })
    }
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Filter {
    #[default]
    Linear,
    Nearest,
}

/// Image bytes that serialize as base64 text.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Base64Bytes(pub Vec<u8>);

impl fmt::Debug for Base64Bytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Base64Bytes({} bytes)", self.0.len())
    }
}

impl Serialize for Base64Bytes {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&base64::engine::general_purpose::STANDARD.encode(&self.0))
    }
}

impl<'de> Deserialize<'de> for Base64Bytes {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        let clean: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        base64::engine::general_purpose::STANDARD
            .decode(clean)
            .map(Base64Bytes)
            .map_err(serde::de::Error::custom)
    }
}

impl schemars::JsonSchema for Base64Bytes {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Base64Bytes".into()
    }

    fn inline_schema() -> bool {
        true
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "type": "string", "contentEncoding": "base64" })
    }
}

/// Where a material's image comes from.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "source", rename_all = "snake_case", deny_unknown_fields)]
pub enum TextureSource {
    /// Absolute, or relative to the directory of the scene file.
    Path { path: String },
    /// The image file's bytes, stored in the scene.
    Embedded {
        format: ImageFormat,
        data: Base64Bytes,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Material {
    pub id: MaterialId,
    /// Unique in the document; faces refer to a material by this name.
    pub name: String,
    pub texture: Option<TextureSource>,
    /// RGBA multiplier on the image, or the flat colour without one.
    pub color: [f32; 4],
    pub emissive: [f32; 3],
    pub emissive_strength: f32,
    pub roughness: f32,
    pub metallic: f32,
    pub filter: Filter,
    pub double_sided: bool,
    pub unlit: bool,
    pub nodraw: bool,
    #[schemars(schema_with = "crate::property::extra_schema")]
    pub extra: Extra,
}

impl Material {
    pub fn new(id: MaterialId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            texture: None,
            color: [1.0, 1.0, 1.0, 1.0],
            emissive: [0.0; 3],
            emissive_strength: 0.0,
            roughness: 1.0,
            metallic: 0.0,
            filter: Filter::Linear,
            double_sided: false,
            unlit: false,
            nodraw: false,
            extra: Extra::new(),
        }
    }
}
