use std::collections::{BTreeMap, HashMap};

use bytemuck::{Pod, Zeroable};
use glam::DVec3;
use level_core::{Document, ObjectId};
use level_geom::mesh::triangulate;
use level_geom::winding::newell;
use level_geom::{Aabb, brush_geometry};

use crate::color::{material_rgb8, to_f32};

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub color: [f32; 4],
}

impl Vertex {
    pub fn new(p: DVec3, color: [f32; 3]) -> Self {
        Self {
            pos: [p.x as f32, p.y as f32, p.z as f32],
            color: [color[0], color[1], color[2], 1.0],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeStyle {
    Dark,
    Colored,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shading {
    Lit,
    Flat,
}

pub struct Drawn {
    pub id: ObjectId,
    pub bounds: Aabb,
}

#[derive(Default)]
pub struct Scene {
    pub tris: Vec<Vertex>,
    pub edges: Vec<Vertex>,
    pub highlight_edges: Vec<Vertex>,
    pub drawn: Vec<Drawn>,
    pub warnings: Vec<String>,
}

pub struct SceneOptions<'a> {
    pub shading: Shading,
    pub edges: EdgeStyle,
    pub draw_faces: bool,
    pub tint: &'a HashMap<ObjectId, [f32; 3]>,
}

const LIGHT: DVec3 = DVec3::new(0.35, -0.5, 0.8);
pub const HIGHLIGHT_COLOR: [f32; 3] = [1.0, 0.85, 0.2];
pub const SELECTION_COLOR: [f32; 3] = [0.2, 0.85, 1.0];
const DARK_EDGE_SCALE: f32 = 0.25;
const ENTITY_HALF: f64 = 8.0;

struct Poly {
    pts: Vec<DVec3>,
    keys: Vec<usize>,
    tris: Vec<[usize; 3]>,
    normal: DVec3,
    rgb: [u8; 3],
}

fn shade(rgb: [f32; 3], normal: DVec3, shading: Shading) -> [f32; 3] {
    match shading {
        Shading::Flat => rgb,
        Shading::Lit => {
            let lambert = normal.dot(LIGHT.normalize()).abs() as f32;
            let k = 0.55 + 0.45 * lambert;
            rgb.map(|c| c * k)
        }
    }
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [0, 1, 2].map(|i| a[i] * (1.0 - t) + b[i] * t)
}

fn emit(scene: &mut Scene, polys: &[Poly], tint: Option<[f32; 3]>, opts: &SceneOptions) {
    for poly in polys {
        let mut base = to_f32(poly.rgb);
        if let Some(tint) = tint {
            base = mix(base, tint, 0.5);
        }
        if opts.draw_faces {
            let color = shade(base, poly.normal, opts.shading);
            for t in &poly.tris {
                for &i in t {
                    scene.tris.push(Vertex::new(poly.pts[i], color));
                }
            }
        }
    }

    let mut edges: BTreeMap<(usize, usize), Vec<usize>> = BTreeMap::new();
    for (pi, poly) in polys.iter().enumerate() {
        let n = poly.keys.len();
        for i in 0..n {
            let (a, b) = (poly.keys[i], poly.keys[(i + 1) % n]);
            edges.entry((a.min(b), a.max(b))).or_default().push(pi);
        }
    }
    let position = |key: usize| -> DVec3 {
        polys
            .iter()
            .find_map(|p| p.keys.iter().position(|&k| k == key).map(|i| p.pts[i]))
            .expect("edge keys come from the polygons")
    };
    for (&(a, b), users) in &edges {
        let feature = match users.as_slice() {
            [p, q] => {
                polys[*p].normal.dot(polys[*q].normal) < 0.99999 || polys[*p].rgb != polys[*q].rgb
            }
            _ => true,
        };
        if !feature {
            continue;
        }
        let rgb = to_f32(polys[users[0]].rgb);
        if opts.edges != EdgeStyle::None {
            let color = match opts.edges {
                EdgeStyle::Dark => rgb.map(|c| c * DARK_EDGE_SCALE),
                _ => rgb,
            };
            scene.edges.push(Vertex::new(position(a), color));
            scene.edges.push(Vertex::new(position(b), color));
        }
        if let Some(tint) = tint {
            scene.highlight_edges.push(Vertex::new(position(a), tint));
            scene.highlight_edges.push(Vertex::new(position(b), tint));
        }
    }
}

fn box_polys(bounds: &Aabb, rgb: [u8; 3]) -> Vec<Poly> {
    let c = bounds.corners();
    let faces: [([usize; 4], DVec3); 6] = [
        ([0, 3, 2, 1], DVec3::NEG_Z),
        ([4, 5, 6, 7], DVec3::Z),
        ([0, 1, 5, 4], DVec3::NEG_Y),
        ([3, 7, 6, 2], DVec3::Y),
        ([0, 4, 7, 3], DVec3::NEG_X),
        ([1, 2, 6, 5], DVec3::X),
    ];
    faces
        .iter()
        .map(|(idx, normal)| Poly {
            pts: idx.iter().map(|&i| c[i]).collect(),
            keys: idx.to_vec(),
            tris: vec![[0, 1, 2], [0, 2, 3]],
            normal: *normal,
            rgb,
        })
        .collect()
}

pub fn build_scene(doc: &Document, opts: &SceneOptions) -> Scene {
    let mut scene = Scene::default();

    for brush in doc.brushes.values() {
        let id = ObjectId::from(brush.id);
        let geo = match brush_geometry(&brush.brush) {
            Ok(g) => g,
            Err(e) => {
                scene.warnings.push(format!("{id} skipped: {e}"));
                continue;
            }
        };
        let polys: Vec<Poly> = geo
            .faces
            .iter()
            .map(|f| {
                let material = brush
                    .brush
                    .faces
                    .iter()
                    .find(|b| b.id == f.face)
                    .map_or("", |b| b.attrs.material.as_str());
                let n = f.loop_.len();
                Poly {
                    pts: geo.face_positions(f),
                    keys: f.loop_.clone(),
                    tris: (1..n.saturating_sub(1)).map(|i| [0, i, i + 1]).collect(),
                    normal: f.plane.normal,
                    rgb: material_rgb8(material),
                }
            })
            .collect();
        if let Some(bounds) = Aabb::from_points(geo.vertices.iter().copied()) {
            scene.drawn.push(Drawn { id, bounds });
        }
        emit(&mut scene, &polys, opts.tint.get(&id).copied(), opts);
    }

    for mesh in doc.meshes.values() {
        let id = ObjectId::from(mesh.id);
        let index: HashMap<_, _> = mesh
            .mesh
            .vertices
            .iter()
            .enumerate()
            .map(|(i, v)| (v.id, i))
            .collect();
        let mut polys = Vec::new();
        for face in &mesh.mesh.faces {
            let keys: Option<Vec<usize>> =
                face.verts.iter().map(|v| index.get(v).copied()).collect();
            let Some(keys) = keys else {
                scene
                    .warnings
                    .push(format!("{id}/f{} skipped: missing vertex", face.id.0));
                continue;
            };
            let pts: Vec<DVec3> = keys.iter().map(|&k| mesh.mesh.vertices[k].pos).collect();
            let normal = newell(&pts).try_normalize().unwrap_or(DVec3::Z);
            polys.push(Poly {
                tris: triangulate(&pts),
                pts,
                keys,
                normal,
                rgb: material_rgb8(&face.attrs.material),
            });
        }
        if let Some(bounds) = Aabb::from_points(mesh.mesh.vertices.iter().map(|v| v.pos)) {
            scene.drawn.push(Drawn { id, bounds });
        }
        emit(&mut scene, &polys, opts.tint.get(&id).copied(), opts);
    }

    for entity in doc.entities.values() {
        let Some(origin) = entity.origin else {
            continue;
        };
        let id = ObjectId::from(entity.id);
        let bounds = Aabb {
            min: origin - DVec3::splat(ENTITY_HALF),
            max: origin + DVec3::splat(ENTITY_HALF),
        };
        let polys = box_polys(&bounds, material_rgb8(&entity.classname));
        scene.drawn.push(Drawn { id, bounds });
        emit(&mut scene, &polys, opts.tint.get(&id).copied(), opts);
    }

    scene
}
