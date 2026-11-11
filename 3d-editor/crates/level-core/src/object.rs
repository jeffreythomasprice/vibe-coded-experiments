use std::fmt;
use std::str::FromStr;

use glam::DVec3;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{
    Brush, BrushId, CameraId, EntityId, Extra, GroupId, IdParseError, LayerId, Mesh, MeshId,
    Properties,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ObjectId {
    Brush(BrushId),
    Mesh(MeshId),
    Entity(EntityId),
    Group(GroupId),
    Layer(LayerId),
    Camera(CameraId),
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ObjectKind {
    Brush,
    Mesh,
    Entity,
    Group,
    Layer,
    Camera,
}

impl ObjectId {
    pub fn number(&self) -> u64 {
        match self {
            ObjectId::Brush(i) => i.0,
            ObjectId::Mesh(i) => i.0,
            ObjectId::Entity(i) => i.0,
            ObjectId::Group(i) => i.0,
            ObjectId::Layer(i) => i.0,
            ObjectId::Camera(i) => i.0,
        }
    }

    pub fn kind(&self) -> ObjectKind {
        match self {
            ObjectId::Brush(_) => ObjectKind::Brush,
            ObjectId::Mesh(_) => ObjectKind::Mesh,
            ObjectId::Entity(_) => ObjectKind::Entity,
            ObjectId::Group(_) => ObjectKind::Group,
            ObjectId::Layer(_) => ObjectKind::Layer,
            ObjectId::Camera(_) => ObjectKind::Camera,
        }
    }
}

impl fmt::Display for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ObjectId::Brush(i) => i.fmt(f),
            ObjectId::Mesh(i) => i.fmt(f),
            ObjectId::Entity(i) => i.fmt(f),
            ObjectId::Group(i) => i.fmt(f),
            ObjectId::Layer(i) => i.fmt(f),
            ObjectId::Camera(i) => i.fmt(f),
        }
    }
}

impl FromStr for ObjectId {
    type Err = IdParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.chars().next() {
            None => Err(IdParseError::Empty),
            Some('b') => s.parse().map(ObjectId::Brush),
            Some('m') => s.parse().map(ObjectId::Mesh),
            Some('e') => s.parse().map(ObjectId::Entity),
            Some('g') => s.parse().map(ObjectId::Group),
            Some('l') => s.parse().map(ObjectId::Layer),
            Some('c') => s.parse().map(ObjectId::Camera),
            Some(_) => Err(IdParseError::UnknownPrefix),
        }
    }
}

impl schemars::JsonSchema for ObjectId {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ObjectId".into()
    }

    fn inline_schema() -> bool {
        true
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "pattern": "^[bmegilc][0-9]+$",
            "description": "Object id: b brush, m mesh, e entity, g group, l layer, c camera",
            "examples": ["b12"],
        })
    }
}

impl Serialize for ObjectId {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for ObjectId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

macro_rules! from_id {
    ($variant:ident, $id:ty) => {
        impl From<$id> for ObjectId {
            fn from(id: $id) -> Self {
                ObjectId::$variant(id)
            }
        }
    };
}

from_id!(Brush, BrushId);
from_id!(Mesh, MeshId);
from_id!(Entity, EntityId);
from_id!(Group, GroupId);
from_id!(Layer, LayerId);
from_id!(Camera, CameraId);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BrushObject {
    pub id: BrushId,
    pub layer: LayerId,
    pub group: Option<GroupId>,
    pub entity: Option<EntityId>,
    pub brush: Brush,
    #[schemars(schema_with = "crate::property::extra_schema")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MeshObject {
    pub id: MeshId,
    pub layer: LayerId,
    pub group: Option<GroupId>,
    pub entity: Option<EntityId>,
    pub mesh: Mesh,
    #[schemars(schema_with = "crate::property::extra_schema")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Entity {
    pub id: EntityId,
    pub classname: String,
    pub layer: LayerId,
    pub group: Option<GroupId>,
    #[schemars(with = "Option<[f64; 3]>")]
    pub origin: Option<DVec3>,
    #[schemars(with = "[f64; 3]")]
    pub angles: DVec3,
    pub properties: Properties,
    #[schemars(schema_with = "crate::property::extra_schema")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Group {
    pub id: GroupId,
    pub name: String,
    pub layer: LayerId,
    pub parent: Option<GroupId>,
    #[schemars(schema_with = "crate::property::extra_schema")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Layer {
    pub id: LayerId,
    pub name: String,
    #[schemars(schema_with = "crate::property::extra_schema")]
    pub extra: Extra,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OrthoView {
    Top,
    Front,
    Side,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Projection {
    Perspective { fov: f64 },
    Orthographic { view: OrthoView, zoom: f64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Camera {
    pub id: CameraId,
    pub name: String,
    #[schemars(with = "[f64; 3]")]
    pub position: DVec3,
    pub yaw: f64,
    pub pitch: f64,
    pub projection: Projection,
    #[schemars(schema_with = "crate::property::extra_schema")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Object {
    Brush(BrushObject),
    Mesh(MeshObject),
    Entity(Entity),
    Group(Group),
    Layer(Layer),
    Camera(Camera),
}

impl Object {
    pub fn id(&self) -> ObjectId {
        match self {
            Object::Brush(o) => o.id.into(),
            Object::Mesh(o) => o.id.into(),
            Object::Entity(o) => o.id.into(),
            Object::Group(o) => o.id.into(),
            Object::Layer(o) => o.id.into(),
            Object::Camera(o) => o.id.into(),
        }
    }

    pub fn layer(&self) -> Option<LayerId> {
        match self {
            Object::Brush(o) => Some(o.layer),
            Object::Mesh(o) => Some(o.layer),
            Object::Entity(o) => Some(o.layer),
            Object::Group(o) => Some(o.layer),
            Object::Layer(_) | Object::Camera(_) => None,
        }
    }

    pub fn group(&self) -> Option<GroupId> {
        match self {
            Object::Brush(o) => o.group,
            Object::Mesh(o) => o.group,
            Object::Entity(o) => o.group,
            Object::Group(o) => o.parent,
            Object::Layer(_) | Object::Camera(_) => None,
        }
    }

    pub fn entity(&self) -> Option<EntityId> {
        match self {
            Object::Brush(o) => o.entity,
            Object::Mesh(o) => o.entity,
            _ => None,
        }
    }
}
