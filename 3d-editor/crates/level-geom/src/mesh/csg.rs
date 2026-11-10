use std::collections::HashMap;

use glam::DVec3;
use level_core::{FaceAttrs, FaceId, Mesh, MeshFace, MeshVertex, Plane, VertexId};

use crate::mesh::triangulate::triangulate;
use crate::mesh::validate::{MeshIssue, validate};
use crate::mesh::weld::weld;
use crate::plane::{Side, classify_distance_eps, flip, signed_distance};
use crate::tolerance::{AREA_EPS, CSG_EPS, WELD_EPS, Welder, snap_vec};
use crate::winding::{area, is_sliver, newell};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CsgError {
    #[error("first operand is not a closed, consistently wound mesh")]
    FirstNotClosed,
    #[error("second operand is not a closed, consistently wound mesh")]
    SecondNotClosed,
    #[error("the result is not watertight at the working tolerance; nudge an operand and retry")]
    InvalidResult,
}

#[derive(Clone)]
struct Polygon {
    verts: Vec<DVec3>,
    plane: Plane,
    attrs: usize,
}

impl Polygon {
    fn flip(&mut self) {
        self.verts.reverse();
        self.plane = flip(&self.plane);
    }
}

#[derive(Default)]
struct Node {
    plane: Option<Plane>,
    front: Option<Box<Node>>,
    back: Option<Box<Node>>,
    polygons: Vec<Polygon>,
}

fn split(
    plane: &Plane,
    poly: Polygon,
    coplanar_front: &mut Vec<Polygon>,
    coplanar_back: &mut Vec<Polygon>,
    front: &mut Vec<Polygon>,
    back: &mut Vec<Polygon>,
) {
    let sides: Vec<Side> = poly
        .verts
        .iter()
        .map(|&v| classify_distance_eps(signed_distance(plane, v), CSG_EPS))
        .collect();
    let has_front = sides.contains(&Side::Front);
    let has_back = sides.contains(&Side::Back);
    match (has_front, has_back) {
        (false, false) => {
            if plane.normal.dot(poly.plane.normal) > 0.0 {
                coplanar_front.push(poly)
            } else {
                coplanar_back.push(poly)
            }
        }
        (true, false) => front.push(poly),
        (false, true) => back.push(poly),
        (true, true) => {
            let mut f = Vec::new();
            let mut b = Vec::new();
            let n = poly.verts.len();
            for i in 0..n {
                let j = (i + 1) % n;
                let (vi, vj) = (poly.verts[i], poly.verts[j]);
                if sides[i] != Side::Back {
                    f.push(vi);
                }
                if sides[i] != Side::Front {
                    b.push(vi);
                }
                if matches!(
                    (sides[i], sides[j]),
                    (Side::Front, Side::Back) | (Side::Back, Side::Front)
                ) {
                    let t = (plane.dist - plane.normal.dot(vi)) / plane.normal.dot(vj - vi);
                    let v = vi + (vj - vi) * t;
                    f.push(v);
                    b.push(v);
                }
            }
            if f.len() >= 3 {
                front.push(Polygon {
                    verts: f,
                    plane: poly.plane,
                    attrs: poly.attrs,
                });
            }
            if b.len() >= 3 {
                back.push(Polygon {
                    verts: b,
                    plane: poly.plane,
                    attrs: poly.attrs,
                });
            }
        }
    }
}

impl Node {
    fn new(polygons: Vec<Polygon>) -> Self {
        let mut n = Node::default();
        n.build(polygons);
        n
    }

    fn invert(&mut self) {
        for p in &mut self.polygons {
            p.flip();
        }
        if let Some(pl) = &mut self.plane {
            *pl = flip(pl);
        }
        if let Some(f) = &mut self.front {
            f.invert();
        }
        if let Some(b) = &mut self.back {
            b.invert();
        }
        std::mem::swap(&mut self.front, &mut self.back);
    }

    fn clip_polygons(&self, polys: Vec<Polygon>) -> Vec<Polygon> {
        let Some(plane) = &self.plane else {
            return polys;
        };
        let mut front = Vec::new();
        let mut back = Vec::new();
        let mut cf = Vec::new();
        let mut cb = Vec::new();
        for p in polys {
            split(plane, p, &mut cf, &mut cb, &mut front, &mut back);
        }
        front.append(&mut cf);
        back.append(&mut cb);
        let mut front = match &self.front {
            Some(n) => n.clip_polygons(front),
            None => front,
        };
        let back = match &self.back {
            Some(n) => n.clip_polygons(back),
            None => Vec::new(),
        };
        front.extend(back);
        front
    }

    fn clip_to(&mut self, other: &Node) {
        self.polygons = other.clip_polygons(std::mem::take(&mut self.polygons));
        if let Some(f) = &mut self.front {
            f.clip_to(other);
        }
        if let Some(b) = &mut self.back {
            b.clip_to(other);
        }
    }

    fn all_polygons(&self) -> Vec<Polygon> {
        let mut out = self.polygons.clone();
        if let Some(f) = &self.front {
            out.extend(f.all_polygons());
        }
        if let Some(b) = &self.back {
            out.extend(b.all_polygons());
        }
        out
    }

    fn build(&mut self, polys: Vec<Polygon>) {
        let mut polys = polys.into_iter();
        if self.plane.is_none() {
            let Some(splitter) = polys.next() else {
                return;
            };
            self.plane = Some(splitter.plane);
            self.polygons.push(splitter);
        }
        let plane = self.plane.expect("plane set above");
        let mut front = Vec::new();
        let mut back = Vec::new();
        let mut cf = Vec::new();
        let mut cb = Vec::new();
        for p in polys {
            split(&plane, p, &mut cf, &mut cb, &mut front, &mut back);
        }
        self.polygons.append(&mut cf);
        self.polygons.append(&mut cb);
        if !front.is_empty() {
            self.front.get_or_insert_with(Default::default).build(front);
        }
        if !back.is_empty() {
            self.back.get_or_insert_with(Default::default).build(back);
        }
    }
}

fn is_watertight(mesh: &Mesh) -> bool {
    validate(mesh).iter().all(|i| match i {
        MeshIssue::DanglingVertex { .. }
        | MeshIssue::FaceTooSmall { .. }
        | MeshIssue::DuplicateVertexInFace { .. }
        | MeshIssue::OpenEdge { .. }
        | MeshIssue::InconsistentWinding { .. } => false,
        MeshIssue::InsideOut => false,
        MeshIssue::NonManifoldEdge {
            forward, backward, ..
        } => forward == backward,
        _ => true,
    })
}

fn require_closed(mesh: &Mesh, err: CsgError) -> Result<(), CsgError> {
    if is_watertight(mesh) {
        Ok(())
    } else {
        Err(err)
    }
}

fn to_polygons(mesh: &Mesh, attrs: &mut Vec<FaceAttrs>) -> Vec<Polygon> {
    let mut out = Vec::new();
    for f in &mesh.faces {
        let poly: Vec<DVec3> = f.verts.iter().filter_map(|&v| mesh.position(v)).collect();
        let attr_idx = attrs.len();
        attrs.push(f.attrs.clone());
        let face_normal = newell(&poly);
        if face_normal.length() <= AREA_EPS {
            continue;
        }
        let normal = face_normal.normalize();
        let dist = poly.iter().map(|p| normal.dot(*p)).sum::<f64>() / poly.len() as f64;
        let plane = Plane { normal, dist };
        for [a, b, c] in triangulate(&poly) {
            let tri = vec![poly[a], poly[b], poly[c]];
            let n = (tri[1] - tri[0]).cross(tri[2] - tri[0]);
            if n.dot(face_normal) <= 0.0 || is_sliver(&tri, CSG_EPS) {
                continue;
            }
            out.push(Polygon {
                plane,
                verts: tri,
                attrs: attr_idx,
            });
        }
    }
    out
}

fn from_polygons(polys: Vec<Polygon>, attrs: &[FaceAttrs]) -> Mesh {
    let mut mesh = Mesh::default();
    let mut welder = Welder::default();
    let mut faces: Vec<(Vec<VertexId>, usize)> = Vec::new();
    for p in polys {
        if area(&p.verts) <= AREA_EPS {
            continue;
        }
        let verts = p
            .verts
            .iter()
            .map(|&v| {
                let (i, added) = welder.insert(v);
                if added {
                    mesh.vertices.push(MeshVertex {
                        id: VertexId(i as u32 + 1),
                        pos: snap_vec(v),
                    });
                }
                VertexId(i as u32 + 1)
            })
            .collect();
        faces.push((verts, p.attrs));
    }
    for (i, (verts, a)) in faces.into_iter().enumerate() {
        mesh.faces.push(MeshFace {
            id: FaceId(i as u32 + 1),
            verts,
            attrs: attrs[a].clone(),
        });
    }
    weld(&mut mesh);
    repair_t_junctions(&mut mesh);
    let positions: HashMap<VertexId, DVec3> = mesh.vertices.iter().map(|v| (v.id, v.pos)).collect();
    mesh.faces.retain(|f| {
        let poly: Vec<DVec3> = f.verts.iter().map(|v| positions[v]).collect();
        newell(&poly).length() > AREA_EPS
    });
    weld(&mut mesh);
    mesh
}

pub fn repair_t_junctions(mesh: &mut Mesh) {
    let verts: Vec<(VertexId, DVec3)> = mesh.vertices.iter().map(|v| (v.id, v.pos)).collect();
    for f in &mut mesh.faces {
        let n = f.verts.len();
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let a = f.verts[i];
            let b = f.verts[(i + 1) % n];
            out.push(a);
            let (Some(pa), Some(pb)) = (
                verts.iter().find(|v| v.0 == a).map(|v| v.1),
                verts.iter().find(|v| v.0 == b).map(|v| v.1),
            ) else {
                continue;
            };
            let d = pb - pa;
            let len2 = d.length_squared();
            if len2 == 0.0 {
                continue;
            }
            let mut inserts: Vec<(f64, VertexId)> = Vec::new();
            for &(id, p) in &verts {
                if id == a || id == b {
                    continue;
                }
                let t = (p - pa).dot(d) / len2;
                let margin = WELD_EPS / len2.sqrt();
                if t <= margin || t >= 1.0 - margin {
                    continue;
                }
                if (p - (pa + d * t)).length() <= WELD_EPS {
                    inserts.push((t, id));
                }
            }
            inserts.sort_by(|x, y| x.0.total_cmp(&y.0));
            out.extend(inserts.into_iter().map(|(_, id)| id));
        }
        f.verts = out;
    }
}

fn run(a: &Mesh, b: &Mesh, op: impl FnOnce(&mut Node, &mut Node)) -> Result<Mesh, CsgError> {
    let mut attrs = Vec::new();
    let pa = to_polygons(a, &mut attrs);
    let pb = to_polygons(b, &mut attrs);
    let mut na = Node::new(pa);
    let mut nb = Node::new(pb);
    op(&mut na, &mut nb);
    let result = from_polygons(na.all_polygons(), &attrs);
    if is_watertight(&result) {
        Ok(result)
    } else {
        Err(CsgError::InvalidResult)
    }
}

pub fn union(a: &Mesh, b: &Mesh) -> Result<Mesh, CsgError> {
    require_closed(a, CsgError::FirstNotClosed)?;
    require_closed(b, CsgError::SecondNotClosed)?;
    if a.faces.is_empty() {
        return Ok(b.clone());
    }
    if b.faces.is_empty() {
        return Ok(a.clone());
    }
    run(a, b, |a, b| {
        a.clip_to(b);
        b.clip_to(a);
        b.invert();
        b.clip_to(a);
        b.invert();
        a.build(b.all_polygons());
    })
}

pub fn subtract(a: &Mesh, b: &Mesh) -> Result<Mesh, CsgError> {
    require_closed(a, CsgError::FirstNotClosed)?;
    require_closed(b, CsgError::SecondNotClosed)?;
    if a.faces.is_empty() || b.faces.is_empty() {
        return Ok(a.clone());
    }
    run(a, b, |a, b| {
        a.invert();
        a.clip_to(b);
        b.clip_to(a);
        b.invert();
        b.clip_to(a);
        b.invert();
        a.build(b.all_polygons());
        a.invert();
    })
}

pub fn intersect(a: &Mesh, b: &Mesh) -> Result<Mesh, CsgError> {
    require_closed(a, CsgError::FirstNotClosed)?;
    require_closed(b, CsgError::SecondNotClosed)?;
    if a.faces.is_empty() || b.faces.is_empty() {
        return Ok(Mesh::default());
    }
    run(a, b, |a, b| {
        a.invert();
        b.clip_to(a);
        b.invert();
        a.clip_to(b);
        b.clip_to(a);
        a.build(b.all_polygons());
        a.invert();
    })
}
