use glam::DVec3;
use serde::{Deserialize, Serialize};

use crate::{FaceAttrs, FaceId, VertexId};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeshVertex {
    pub id: VertexId,
    pub pos: DVec3,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeshFace {
    pub id: FaceId,
    pub verts: Vec<VertexId>,
    pub attrs: FaceAttrs,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Mesh {
    pub vertices: Vec<MeshVertex>,
    pub faces: Vec<MeshFace>,
}

impl Mesh {
    pub fn next_vertex_id(&self) -> VertexId {
        VertexId(
            self.vertices
                .iter()
                .map(|v| v.id.0)
                .max()
                .map_or(1, |m| m + 1),
        )
    }

    pub fn next_face_id(&self) -> FaceId {
        FaceId(self.faces.iter().map(|f| f.id.0).max().map_or(1, |m| m + 1))
    }

    pub fn vertex(&self, id: VertexId) -> Option<&MeshVertex> {
        self.vertices.iter().find(|v| v.id == id)
    }

    pub fn position(&self, id: VertexId) -> Option<DVec3> {
        self.vertex(id).map(|v| v.pos)
    }
}
