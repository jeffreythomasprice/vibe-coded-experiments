use glam::DVec3;
use serde::{Deserialize, Serialize};

use crate::{FaceAttrs, FaceId};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrushFace {
    pub id: FaceId,
    #[serde(rename = "plane")]
    pub points: [DVec3; 3],
    pub attrs: FaceAttrs,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Brush {
    pub faces: Vec<BrushFace>,
}

impl Brush {
    pub fn next_face_id(&self) -> FaceId {
        FaceId(self.faces.iter().map(|f| f.id.0).max().map_or(1, |m| m + 1))
    }
}
