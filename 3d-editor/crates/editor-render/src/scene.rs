use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use editor_cmd::ColorMode;
use editor_core::{Document, EntityId, FaceAttrs, FaceId, ObjectId};
use editor_geom::mesh::triangulate;
use editor_geom::winding::newell;
use editor_geom::{Aabb, brush_geometry};
use glam::DVec3;

use crate::color::{material_rgb8, to_f32};
use crate::texture::{RenderMaterial, TextureCache};

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub color: [f32; 4],
    pub uv: [f32; 2],
}

impl Vertex {
    pub fn new(p: DVec3, color: [f32; 3]) -> Self {
        Self::textured(p, color, [0.0, 0.0])
    }

    pub fn textured(p: DVec3, color: [f32; 3], uv: [f32; 2]) -> Self {
        Self {
            pos: [p.x as f32, p.y as f32, p.z as f32],
            color: [color[0], color[1], color[2], 1.0],
            uv,
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

/// Triangles drawn with one material; `None` is the plain flat-coloured kind.
pub struct FaceBatch {
    pub material: Option<Arc<RenderMaterial>>,
    /// The entity whose geometry this is; set only when `SceneOptions::split_owned` is on.
    pub owner: Option<EntityId>,
    pub verts: Vec<Vertex>,
}

#[derive(Default)]
pub struct Scene {
    pub faces: Vec<FaceBatch>,
    /// Batches of entity-owned geometry, kept apart so an entity can be moved without touching the rest.
    pub owned: Vec<FaceBatch>,
    pub edges: Vec<Vertex>,
    pub highlight_edges: Vec<Vertex>,
    pub drawn: Vec<Drawn>,
    pub id_tris: Vec<Vertex>,
    pub legend_faces: BTreeMap<String, [u8; 3]>,
    pub legend_entities: BTreeMap<String, ([u8; 3], &'static str)>,
    pub legend_textures: BTreeSet<String>,
    pub warnings: Vec<String>,
}

impl Scene {
    fn batch(
        &mut self,
        material: &Option<Arc<RenderMaterial>>,
        owner: Option<EntityId>,
    ) -> &mut Vec<Vertex> {
        let list = if owner.is_some() {
            &mut self.owned
        } else {
            &mut self.faces
        };
        let at = list.iter().position(|b| {
            b.owner == owner
                && match (&b.material, material) {
                    (None, None) => true,
                    (Some(a), Some(b)) => a.id == b.id,
                    _ => false,
                }
        });
        let at = at.unwrap_or_else(|| {
            list.push(FaceBatch {
                material: material.clone(),
                owner,
                verts: Vec::new(),
            });
            list.len() - 1
        });
        &mut list[at].verts
    }
}

/// Where material images come from; without it faces keep their flat colours.
#[derive(Clone, Copy)]
pub struct TextureInputs<'a> {
    pub cache: &'a TextureCache,
    pub scene_dir: Option<&'a Path>,
}

/// How a defined entity class is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ComponentLook {
    pub rgb: [u8; 3],
    /// Marker box relative to the origin; `None` keeps the built-in marker.
    pub size: Option<(DVec3, DVec3)>,
}

pub struct SceneOptions<'a> {
    pub looks: &'a HashMap<String, ComponentLook>,
    pub shading: Shading,
    pub edges: EdgeStyle,
    pub draw_faces: bool,
    pub tint: &'a HashMap<ObjectId, [f32; 3]>,
    pub hidden: &'a HashSet<ObjectId>,
    pub colors: ColorMode,
    pub overrides: &'a HashMap<String, [u8; 3]>,
    pub want_ids: bool,
    pub textures: Option<TextureInputs<'a>>,
    pub face_tint: &'a BTreeSet<(ObjectId, FaceId)>,
    /// Draw a marker for each point entity.
    pub markers: bool,
    /// Keep entity-owned geometry in `Scene::owned`, so it can be posed.
    pub split_owned: bool,
}

pub fn id_color(code: u32) -> [f32; 3] {
    [0, 8, 16].map(|shift| ((code >> shift) & 0xff) as f32 / 255.0)
}

pub fn decode_id(px: [u8; 4]) -> u32 {
    u32::from(px[0]) | u32::from(px[1]) << 8 | u32::from(px[2]) << 16
}

const UP_RGB: [u8; 3] = [96, 170, 96];
const DOWN_RGB: [u8; 3] = [90, 120, 190];
const SIDE_RGB: [u8; 3] = [190, 170, 130];
const LIGHT_RGB: [u8; 3] = [255, 214, 64];

fn face_color(
    opts: &SceneOptions,
    legend: &mut BTreeMap<String, [u8; 3]>,
    material: &str,
    normal: DVec3,
    layer: &str,
) -> [u8; 3] {
    let (rgb, name) = if let Some(&rgb) = opts.overrides.get(material) {
        (rgb, material.to_string())
    } else {
        match opts.colors {
            ColorMode::Material => (material_rgb8(material), material.to_string()),
            ColorMode::Orientation if normal.z > 0.5 => (UP_RGB, "faces up (floors)".to_string()),
            ColorMode::Orientation if normal.z < -0.5 => {
                (DOWN_RGB, "faces down (ceilings)".to_string())
            }
            ColorMode::Orientation => (SIDE_RGB, "vertical (walls)".to_string()),
            ColorMode::Layer => (material_rgb8(layer), layer.to_string()),
        }
    };
    legend.insert(name, rgb);
    rgb
}

const LIGHT: DVec3 = DVec3::new(0.35, -0.5, 0.8);
pub const HIGHLIGHT_COLOR: [f32; 3] = [1.0, 0.85, 0.2];
pub const SELECTION_COLOR: [f32; 3] = [0.2, 0.85, 1.0];
pub const FACE_SELECTION_COLOR: [f32; 3] = [1.0, 0.45, 0.1];
const DARK_EDGE_SCALE: f32 = 0.25;
const ENTITY_HALF: f64 = 8.0;

struct Poly {
    pts: Vec<DVec3>,
    keys: Vec<usize>,
    tris: Vec<[usize; 3]>,
    normal: DVec3,
    rgb: [u8; 3],
    material: Option<Arc<RenderMaterial>>,
    uvs: Vec<[f32; 2]>,
    face_selected: bool,
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

fn emit(
    scene: &mut Scene,
    polys: &[Poly],
    tint: Option<[f32; 3]>,
    opts: &SceneOptions,
    code: u32,
    owner: Option<EntityId>,
) {
    let owner = owner.filter(|_| opts.split_owned);
    for poly in polys {
        if opts.want_ids && code != 0 {
            let color = id_color(code);
            for t in &poly.tris {
                for &i in t {
                    scene.id_tris.push(Vertex::new(poly.pts[i], color));
                }
            }
        }
        let mut base = match &poly.material {
            Some(_) => [1.0; 3],
            None => to_f32(poly.rgb),
        };
        if let Some(tint) = tint {
            base = mix(base, tint, 0.5);
        }
        if poly.face_selected {
            base = mix(base, FACE_SELECTION_COLOR, 0.55);
        }
        if opts.draw_faces {
            let shading = match &poly.material {
                Some(m) if m.unlit => Shading::Flat,
                _ => opts.shading,
            };
            let color = shade(base, poly.normal, shading);
            let out = scene.batch(&poly.material, owner);
            for t in &poly.tris {
                for &i in t {
                    let uv = poly.uvs.get(i).copied().unwrap_or([0.0, 0.0]);
                    out.push(Vertex::textured(poly.pts[i], color, uv));
                }
            }
        }
        if poly.face_selected {
            let n = poly.pts.len();
            for i in 0..n {
                scene
                    .highlight_edges
                    .push(Vertex::new(poly.pts[i], FACE_SELECTION_COLOR));
                scene
                    .highlight_edges
                    .push(Vertex::new(poly.pts[(i + 1) % n], FACE_SELECTION_COLOR));
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
            material: None,
            uvs: Vec::new(),
            face_selected: false,
        })
        .collect()
}

/// How one face is drawn: with a material's image and properties, or as a flat colour.
struct FaceLook {
    rgb: [u8; 3],
    material: Option<Arc<RenderMaterial>>,
}

struct Looks<'a> {
    doc: &'a Document,
    opts: &'a SceneOptions<'a>,
    materials: HashMap<String, Option<Arc<RenderMaterial>>>,
}

impl Looks<'_> {
    fn material(&mut self, name: &str, scene: &mut Scene) -> Option<Arc<RenderMaterial>> {
        if name.is_empty() {
            return None;
        }
        if let Some(hit) = self.materials.get(name) {
            return hit.clone();
        }
        let found = self.build(name, scene);
        self.materials.insert(name.to_string(), found.clone());
        found
    }

    fn build(&self, name: &str, scene: &mut Scene) -> Option<Arc<RenderMaterial>> {
        let inputs = self.opts.textures?;
        if self.opts.colors != ColorMode::Material || self.opts.overrides.contains_key(name) {
            return None;
        }
        let material = self.doc.material_by_name(name)?;
        let mut problem = None;
        let built = inputs
            .cache
            .material(material, inputs.scene_dir, &mut problem);
        scene.warnings.extend(problem);
        Some(Arc::new(built))
    }

    fn look(&mut self, scene: &mut Scene, name: &str, normal: DVec3, layer: &str) -> FaceLook {
        match self.material(name, scene) {
            Some(m) => {
                let rgb = if m.texture.is_some() {
                    scene.legend_textures.insert(name.to_string());
                    material_rgb8(name)
                } else {
                    let rgb = [0, 1, 2].map(|i| (m.tint[i] * 255.0).round() as u8);
                    scene.legend_faces.insert(name.to_string(), rgb);
                    rgb
                };
                FaceLook {
                    rgb,
                    material: Some(m),
                }
            }
            None => FaceLook {
                rgb: face_color(self.opts, &mut scene.legend_faces, name, normal, layer),
                material: None,
            },
        }
    }
}

fn face_uvs(look: &FaceLook, attrs: &FaceAttrs, normal: DVec3, pts: &[DVec3]) -> Vec<[f32; 2]> {
    let Some(m) = &look.material else {
        return Vec::new();
    };
    let (w, h) = m.size();
    let map = editor_geom::uv::texel_map(attrs, normal);
    pts.iter()
        .map(|&p| {
            let t = map.texel(p);
            [(t.x / w) as f32, (t.y / h) as f32]
        })
        .collect()
}

/// Looks of the components that set a colour or size; other components do not decide how an entity is drawn.
pub fn component_looks(
    catalog: &editor_cmd::component_defs::Catalog,
) -> HashMap<String, ComponentLook> {
    catalog
        .components()
        .filter(|r| r.def.color.is_some() || r.def.size.is_some())
        .map(|r| {
            let c = &r.def;
            let rgb = c.color.map_or_else(
                || material_rgb8(&c.name),
                |rgb| rgb.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8),
            );
            let size = c
                .size
                .as_ref()
                .map(|s| (DVec3::from(s.min), DVec3::from(s.max)));
            (c.name.to_string(), ComponentLook { rgb, size })
        })
        .collect()
}

pub fn build_scene(doc: &Document, opts: &SceneOptions) -> Scene {
    let mut scene = Scene::default();
    let mut looks = Looks {
        doc,
        opts,
        materials: HashMap::new(),
    };

    for brush in doc.brushes.values() {
        let id = ObjectId::from(brush.id);
        if opts.hidden.contains(&id) {
            continue;
        }
        let layer = doc.layers.get(&brush.layer).map_or("", |l| l.name.as_str());
        let geo = match brush_geometry(&brush.brush) {
            Ok(g) => g,
            Err(e) => {
                scene.warnings.push(format!("{id} skipped: {e}"));
                continue;
            }
        };
        let mut polys: Vec<Poly> = Vec::new();
        for f in &geo.faces {
            let attrs = brush
                .brush
                .faces
                .iter()
                .find(|b| b.id == f.face)
                .map(|b| b.attrs.clone())
                .unwrap_or_default();
            let n = f.loop_.len();
            let pts = geo.face_positions(f);
            let look = looks.look(&mut scene, &attrs.material, f.plane.normal, layer);
            polys.push(Poly {
                uvs: face_uvs(&look, &attrs, f.plane.normal, &pts),
                pts,
                keys: f.loop_.clone(),
                tris: (1..n.saturating_sub(1)).map(|i| [0, i, i + 1]).collect(),
                normal: f.plane.normal,
                rgb: look.rgb,
                material: look.material,
                face_selected: opts.face_tint.contains(&(id, f.face)),
            });
        }
        if let Some(bounds) = Aabb::from_points(geo.vertices.iter().copied()) {
            scene.drawn.push(Drawn { id, bounds });
        }
        let code = scene.drawn.len() as u32;
        emit(
            &mut scene,
            &polys,
            opts.tint.get(&id).copied(),
            opts,
            code,
            brush.entity,
        );
    }

    for mesh in doc.meshes.values() {
        let id = ObjectId::from(mesh.id);
        if opts.hidden.contains(&id) {
            continue;
        }
        let layer = doc.layers.get(&mesh.layer).map_or("", |l| l.name.as_str());
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
            let look = looks.look(&mut scene, &face.attrs.material, normal, layer);
            polys.push(Poly {
                uvs: face_uvs(&look, &face.attrs, normal, &pts),
                tris: triangulate(&pts),
                pts,
                keys,
                normal,
                rgb: look.rgb,
                material: look.material,
                face_selected: opts.face_tint.contains(&(id, face.id)),
            });
        }
        if let Some(bounds) = Aabb::from_points(mesh.mesh.vertices.iter().map(|v| v.pos)) {
            scene.drawn.push(Drawn { id, bounds });
        }
        let code = scene.drawn.len() as u32;
        emit(
            &mut scene,
            &polys,
            opts.tint.get(&id).copied(),
            opts,
            code,
            mesh.entity,
        );
    }

    let owners: std::collections::HashSet<_> = doc
        .brushes
        .values()
        .filter_map(|b| b.entity)
        .chain(doc.meshes.values().filter_map(|m| m.entity))
        .collect();
    for entity in doc.entities.values().filter(|_| opts.markers) {
        if owners.contains(&entity.id) {
            continue;
        }
        let origin = entity.origin;
        let id = ObjectId::from(entity.id);
        if opts.hidden.contains(&id) {
            continue;
        }
        let key = entity
            .components
            .keys()
            .find(|name| opts.looks.contains_key(*name))
            .or_else(|| entity.components.keys().next())
            .map_or("entity", String::as_str);
        let (polys, bounds, rgb, marker) = entity_marker(key, origin, opts.looks.get(key));
        scene.legend_entities.insert(key.to_string(), (rgb, marker));
        scene.drawn.push(Drawn { id, bounds });
        let code = scene.drawn.len() as u32;
        emit(
            &mut scene,
            &polys,
            opts.tint.get(&id).copied(),
            opts,
            code,
            None,
        );
    }

    scene
}

fn entity_marker(
    key: &str,
    origin: DVec3,
    look: Option<&ComponentLook>,
) -> (Vec<Poly>, Aabb, [u8; 3], &'static str) {
    if let Some(ComponentLook {
        rgb,
        size: Some((lo, hi)),
    }) = look
    {
        let bounds = Aabb {
            min: origin + *lo,
            max: origin + *hi,
        };
        return (box_polys(&bounds, *rgb), bounds, *rgb, "class box");
    }
    let tint = look.map(|l| l.rgb);
    if key.starts_with("light") {
        let r = 10.0;
        let pts = [
            DVec3::X,
            DVec3::NEG_X,
            DVec3::Y,
            DVec3::NEG_Y,
            DVec3::Z,
            DVec3::NEG_Z,
        ]
        .map(|d| origin + d * r);
        let mut polys = Vec::new();
        for (sx, sy, sz) in (0..8).map(|i| (i & 1, (i >> 1) & 1, (i >> 2) & 1)) {
            let keys = vec![sx, 2 + sy, 4 + sz];
            let normal = DVec3::new(
                if sx == 0 { 1.0 } else { -1.0 },
                if sy == 0 { 1.0 } else { -1.0 },
                if sz == 0 { 1.0 } else { -1.0 },
            )
            .normalize();
            polys.push(Poly {
                pts: keys.iter().map(|&k| pts[k]).collect(),
                keys,
                tris: vec![[0, 1, 2]],
                normal,
                rgb: tint.unwrap_or(LIGHT_RGB),
                material: None,
                uvs: Vec::new(),
                face_selected: false,
            });
        }
        let bounds = Aabb {
            min: origin - DVec3::splat(r),
            max: origin + DVec3::splat(r),
        };
        return (polys, bounds, tint.unwrap_or(LIGHT_RGB), "octahedron");
    }
    let (half, rgb, marker) = (
        DVec3::splat(ENTITY_HALF),
        tint.unwrap_or_else(|| material_rgb8(key)),
        "box",
    );
    let bounds = Aabb {
        min: origin - half,
        max: origin + half,
    };
    (box_polys(&bounds, rgb), bounds, rgb, marker)
}
