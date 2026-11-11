use std::collections::{BTreeMap, BTreeSet};

use editor_core::{Document, ElementRef, FaceAttrs, FaceId, Object, ObjectId, Part, Plane};
use editor_geom::uv::{self, NOMINAL_SIZE};
use editor_geom::{brush_geometry, winding};
use glam::{DQuat, DVec2, DVec3};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::brush::geometry_error;
use super::results::{self, CornerUv, FaceUv};
use super::util::{brush_or_mesh, non_empty};
use super::{CmdResult, Outcome};
use crate::ctx::Ctx;
use crate::error::CommandError;
use crate::texture;

pub(super) struct FaceInfo {
    pub object: ObjectId,
    pub face: FaceId,
    pub plane: Plane,
    pub corners: Vec<DVec3>,
    pub attrs: FaceAttrs,
}

impl FaceInfo {
    fn element(&self) -> ElementRef {
        ElementRef {
            object: self.object,
            part: Part::Face(self.face),
        }
    }
}

fn loop_plane(points: &[DVec3]) -> Plane {
    let n = winding::newell(points);
    let normal = n.try_normalize().unwrap_or(DVec3::Z);
    let centroid = points.iter().copied().sum::<DVec3>() / points.len().max(1) as f64;
    Plane {
        normal,
        dist: normal.dot(centroid),
    }
}

/// The faces `refs` name, in the order given, with their geometry. A bare object id means all of its faces.
pub(super) fn face_infos(
    doc: &Document,
    refs: &[ElementRef],
) -> Result<Vec<FaceInfo>, CommandError> {
    non_empty("faces", refs)?;
    let mut out: Vec<FaceInfo> = Vec::new();
    let mut seen: BTreeSet<(ObjectId, FaceId)> = BTreeSet::new();
    let mut cache: BTreeMap<ObjectId, Vec<FaceInfo>> = BTreeMap::new();
    for r in refs {
        if let std::collections::btree_map::Entry::Vacant(slot) = cache.entry(r.object) {
            let all = match brush_or_mesh(doc, r.object)? {
                Object::Brush(b) => {
                    let geo = brush_geometry(&b.brush).map_err(|e| geometry_error(r.object, e))?;
                    geo.faces
                        .iter()
                        .filter_map(|g| {
                            let attrs =
                                b.brush.faces.iter().find(|f| f.id == g.face)?.attrs.clone();
                            Some(FaceInfo {
                                object: r.object,
                                face: g.face,
                                plane: g.plane,
                                corners: geo.face_positions(g),
                                attrs,
                            })
                        })
                        .collect()
                }
                Object::Mesh(m) => m
                    .mesh
                    .faces
                    .iter()
                    .filter_map(|f| {
                        let corners: Option<Vec<DVec3>> = f
                            .verts
                            .iter()
                            .map(|&v| m.mesh.vertex(v).map(|x| x.pos))
                            .collect();
                        let corners = corners?;
                        Some(FaceInfo {
                            object: r.object,
                            face: f.id,
                            plane: loop_plane(&corners),
                            corners,
                            attrs: f.attrs.clone(),
                        })
                    })
                    .collect(),
                _ => unreachable!("brush_or_mesh returned another kind"),
            };
            slot.insert(all);
        }
        let all = &cache[&r.object];
        match r.part {
            Part::All => {
                for info in all {
                    if seen.insert((info.object, info.face)) {
                        out.push(clone_info(info));
                    }
                }
            }
            Part::Face(f) => {
                let info = all
                    .iter()
                    .find(|i| i.face == f)
                    .ok_or_else(|| CommandError::not_found(r))?;
                if seen.insert((info.object, info.face)) {
                    out.push(clone_info(info));
                }
            }
            Part::Vertex(_) => {
                return Err(CommandError::invalid_arg(
                    "faces",
                    format!("{r} is a vertex; faces look like b12/f3"),
                ));
            }
        }
    }
    Ok(out)
}

fn clone_info(i: &FaceInfo) -> FaceInfo {
    FaceInfo {
        object: i.object,
        face: i.face,
        plane: i.plane,
        corners: i.corners.clone(),
        attrs: i.attrs.clone(),
    }
}

fn write_attrs(
    ctx: &mut Ctx,
    updates: Vec<(ObjectId, FaceId, FaceAttrs)>,
) -> Result<usize, CommandError> {
    let mut by_object: BTreeMap<ObjectId, Vec<(FaceId, FaceAttrs)>> = BTreeMap::new();
    let count = updates.len();
    for (o, f, a) in updates {
        by_object.entry(o).or_default().push((f, a));
    }
    for (id, list) in by_object {
        let mut object = brush_or_mesh(ctx.doc(), id)?;
        for (face, attrs) in list {
            match &mut object {
                Object::Brush(b) => {
                    if let Some(f) = b.brush.faces.iter_mut().find(|f| f.id == face) {
                        f.attrs = attrs;
                    }
                }
                Object::Mesh(m) => {
                    if let Some(f) = m.mesh.faces.iter_mut().find(|f| f.id == face) {
                        f.attrs = attrs;
                    }
                }
                _ => {}
            }
        }
        ctx.insert(object);
    }
    Ok(count)
}

/// The size of the image a material shows, and why it cannot be read when it cannot.
fn material_size(ctx: &Ctx, name: &str) -> (DVec2, Option<String>, bool) {
    let nominal = DVec2::splat(NOMINAL_SIZE);
    let Some(m) = ctx.doc().material_by_name(name) else {
        return (nominal, None, name.is_empty());
    };
    match &m.texture {
        None => (nominal, None, true),
        Some(source) => match texture::source_size(source, ctx.scene_dir()) {
            Ok(s) => (DVec2::new(s.width as f64, s.height as f64), None, true),
            Err(e) => (nominal, Some(e.to_string()), true),
        },
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GetUv {
    pub faces: Vec<ElementRef>,
}

pub fn get_uv(ctx: &mut Ctx, args: GetUv) -> CmdResult<results::FaceUvList> {
    let infos = face_infos(ctx.doc(), &args.faces)?;
    let faces = infos
        .iter()
        .map(|i| {
            let (size, texture_error, _) = material_size(ctx, &i.attrs.material);
            let map = uv::texel_map(&i.attrs, i.plane.normal);
            let corners: Vec<CornerUv> = i
                .corners
                .iter()
                .map(|&p| {
                    let texel = map.texel(p);
                    CornerUv {
                        position: p,
                        texel,
                        uv: texel / size,
                    }
                })
                .collect();
            let bounds = uv::texel_bounds(&i.attrs, i.plane.normal, &i.corners);
            FaceUv {
                face: i.element(),
                material: i.attrs.material.clone(),
                defined: i.attrs.material.is_empty()
                    || ctx.doc().material_by_name(&i.attrs.material).is_some(),
                size,
                texture_error,
                attrs: i.attrs.clone(),
                normal: i.plane.normal,
                axes: uv::effective_axes(&i.attrs, i.plane.normal),
                corners,
                texel_min: bounds.map(|b| b.0),
                texel_max: bounds.map(|b| b.1),
            }
        })
        .collect();
    Ok(Outcome::new(results::FaceUvList { faces }))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Anchor {
    Start,
    Center,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UvAxis {
    U,
    V,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FitAxes {
    #[default]
    Both,
    U,
    V,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ResetTo {
    /// Axes picked from the dominant axis of the normal, scale 1, no offset or rotation.
    #[default]
    Paraxial,
    /// Axes lying in the face's plane (U runs right as seen from outside, V down), scale 1, no offset or rotation.
    FaceAligned,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum AlignAction {
    /// Scale and shift the image so the face covers `repeat` copies of it (default [1, 1]); rotation and axes are kept.
    Fit {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        repeat: Option<[f64; 2]>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        axes: Option<FitAxes>,
    },
    /// Shift the image so the face's edge (start or end) or middle meets the image's, per axis.
    Justify {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        u: Option<Anchor>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        v: Option<Anchor>,
    },
    /// Mirror the image along one axis while the face keeps showing the same part of it.
    Flip { axis: UvAxis },
    /// Start over from the default alignment.
    Reset {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to: Option<ResetTo>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AlignUv {
    pub faces: Vec<ElementRef>,
    pub action: AlignAction,
    /// Treat the faces as one surface: the first face sets the alignment and the others continue it, so the image runs across all of them.
    #[serde(default)]
    pub as_group: bool,
}

fn sign(x: f64) -> f64 {
    if x < 0.0 { -1.0 } else { 1.0 }
}

fn fit(
    attrs: &FaceAttrs,
    normal: DVec3,
    corners: &[DVec3],
    size: DVec2,
    repeat: Option<[f64; 2]>,
    axes: FitAxes,
) -> Result<FaceAttrs, CommandError> {
    let [rx, ry] = repeat.unwrap_or([1.0, 1.0]);
    if !(rx.is_finite() && ry.is_finite() && rx > 0.0 && ry > 0.0) {
        return Err(CommandError::invalid_arg(
            "action",
            "repeat must be two positive numbers",
        ));
    }
    let unit = FaceAttrs {
        scale: DVec2::new(sign(attrs.scale.x), sign(attrs.scale.y)),
        offset: DVec2::ZERO,
        ..attrs.clone()
    };
    let (lo, hi) = uv::texel_bounds(&unit, normal, corners)
        .ok_or_else(|| CommandError::invalid_arg("faces", "a face has no corners"))?;
    let extent = hi - lo;
    let mut out = attrs.clone();
    let do_u = axes != FitAxes::V && extent.x > 1e-9;
    let do_v = axes != FitAxes::U && extent.y > 1e-9;
    if do_u {
        out.scale.x = sign(attrs.scale.x) * extent.x / (rx * size.x);
    }
    if do_v {
        out.scale.y = sign(attrs.scale.y) * extent.y / (ry * size.y);
    }
    let zero = FaceAttrs {
        offset: DVec2::ZERO,
        ..out.clone()
    };
    let (start, _) = uv::texel_bounds(&zero, normal, corners).expect("corners checked above");
    if do_u {
        out.offset.x = -start.x;
    }
    if do_v {
        out.offset.y = -start.y;
    }
    Ok(out)
}

fn justify(
    attrs: &FaceAttrs,
    normal: DVec3,
    corners: &[DVec3],
    size: DVec2,
    u: Option<Anchor>,
    v: Option<Anchor>,
) -> Result<FaceAttrs, CommandError> {
    let (lo, hi) = uv::texel_bounds(attrs, normal, corners)
        .ok_or_else(|| CommandError::invalid_arg("faces", "a face has no corners"))?;
    let shift = |anchor: Option<Anchor>, lo: f64, hi: f64, size: f64| match anchor {
        None => 0.0,
        Some(Anchor::Start) => -lo,
        Some(Anchor::Center) => size / 2.0 - (lo + hi) / 2.0,
        Some(Anchor::End) => size - hi,
    };
    let mut out = attrs.clone();
    out.offset.x += shift(u, lo.x, hi.x, size.x);
    out.offset.y += shift(v, lo.y, hi.y, size.y);
    Ok(out)
}

fn flip(
    attrs: &FaceAttrs,
    normal: DVec3,
    corners: &[DVec3],
    axis: UvAxis,
) -> Result<FaceAttrs, CommandError> {
    let (lo, hi) = uv::texel_bounds(attrs, normal, corners)
        .ok_or_else(|| CommandError::invalid_arg("faces", "a face has no corners"))?;
    let mut out = attrs.clone();
    match axis {
        UvAxis::U => {
            out.scale.x = -attrs.scale.x;
            out.offset.x = lo.x + hi.x - attrs.offset.x;
        }
        UvAxis::V => {
            out.scale.y = -attrs.scale.y;
            out.offset.y = lo.y + hi.y - attrs.offset.y;
        }
    }
    Ok(out)
}

fn reset(attrs: &FaceAttrs, normal: DVec3, to: ResetTo) -> FaceAttrs {
    FaceAttrs {
        material: attrs.material.clone(),
        axes: match to {
            ResetTo::Paraxial => None,
            ResetTo::FaceAligned => Some(uv::face_aligned_axes(normal)),
        },
        ..FaceAttrs::default()
    }
}

pub fn align_uv(ctx: &mut Ctx, args: AlignUv) -> CmdResult<results::FacesChanged> {
    let infos = face_infos(ctx.doc(), &args.faces)?;
    let groups: Vec<Vec<usize>> = if args.as_group {
        vec![(0..infos.len()).collect()]
    } else {
        (0..infos.len()).map(|i| vec![i]).collect()
    };
    let mut updates = Vec::new();
    for group in groups {
        let r = &infos[group[0]];
        let corners: Vec<DVec3> = group
            .iter()
            .flat_map(|&i| infos[i].corners.iter().copied())
            .collect();
        let (size, _, _) = material_size(ctx, &r.attrs.material);
        let normal = r.plane.normal;
        let aligned = match &args.action {
            AlignAction::Fit { repeat, axes } => fit(
                &r.attrs,
                normal,
                &corners,
                size,
                *repeat,
                axes.unwrap_or_default(),
            )?,
            AlignAction::Justify { u, v } => {
                if u.is_none() && v.is_none() {
                    return Err(CommandError::invalid_arg("action", "give u, v or both"));
                }
                justify(&r.attrs, normal, &corners, size, *u, *v)?
            }
            AlignAction::Flip { axis } => flip(&r.attrs, normal, &corners, *axis)?,
            AlignAction::Reset { to } => reset(&r.attrs, normal, to.unwrap_or_default()),
        };
        let map = uv::texel_map(&aligned, normal);
        for &i in &group {
            let info = &infos[i];
            let attrs = if i == group[0] {
                aligned.clone()
            } else {
                uv::attrs_from_map(&info.attrs, &map, &info.plane)
                    .unwrap_or_else(|| info.attrs.clone())
            };
            updates.push((info.object, info.face, attrs));
        }
    }
    let faces = write_attrs(ctx, updates)?;
    Ok(Outcome::new(results::FacesChanged { faces }))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TransferMode {
    /// Copy the material and the attributes as they are.
    Copy,
    /// Continue the source's image onto the target: across the edge they share (as if folded flat), or by projecting along the source's mapping when they share none.
    #[default]
    Wrap,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransferUv {
    pub from: ElementRef,
    pub to: Vec<ElementRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<TransferMode>,
}

fn shared_edge(a: &FaceInfo, b: &FaceInfo) -> Option<(DVec3, DVec3)> {
    let mut common: Vec<DVec3> = Vec::new();
    for p in &a.corners {
        if b.corners.iter().any(|q| (*q - *p).length() < 1e-6)
            && !common.iter().any(|c| (*c - *p).length() < 1e-6)
        {
            common.push(*p);
        }
    }
    (common.len() >= 2).then(|| (common[0], common[1]))
}

fn wrapped(source: &FaceInfo, target: &FaceInfo) -> Option<FaceAttrs> {
    let m = uv::texel_map(&source.attrs, source.plane.normal);
    let map = match shared_edge(source, target) {
        None => m,
        Some((a, b)) => {
            let axis = (b - a).try_normalize()?;
            let (nt, ns) = (target.plane.normal, source.plane.normal);
            let angle = axis.dot(nt.cross(ns)).atan2(nt.dot(ns));
            let inverse = DQuat::from_axis_angle(axis, angle).inverse();
            let gu = inverse * m.gu;
            let gv = inverse * m.gv;
            uv::TexMap {
                gu,
                gv,
                offset: DVec2::new(
                    m.offset.x + m.gu.dot(a) - gu.dot(a),
                    m.offset.y + m.gv.dot(a) - gv.dot(a),
                ),
            }
        }
    };
    uv::attrs_from_map(&source.attrs, &map, &target.plane)
}

pub fn transfer_uv(ctx: &mut Ctx, args: TransferUv) -> CmdResult<results::FacesChanged> {
    if !matches!(args.from.part, Part::Face(_)) {
        return Err(CommandError::invalid_arg(
            "from",
            "name one face, like b12/f3",
        ));
    }
    let source = face_infos(ctx.doc(), std::slice::from_ref(&args.from))?.remove(0);
    let targets = face_infos(ctx.doc(), &args.to)?;
    let mode = args.mode.unwrap_or_default();
    let mut updates = Vec::new();
    for t in targets
        .iter()
        .filter(|t| !(t.object == source.object && t.face == source.face))
    {
        let attrs = match mode {
            TransferMode::Copy => source.attrs.clone(),
            TransferMode::Wrap => wrapped(&source, t).unwrap_or_else(|| source.attrs.clone()),
        };
        updates.push((t.object, t.face, attrs));
    }
    let faces = write_attrs(ctx, updates)?;
    Ok(Outcome::new(results::FacesChanged { faces }))
}
