use editor_core::{FaceAttrs, Mesh, Plane, UvAxes};
use glam::{DQuat, DVec2, DVec3};

use crate::plane::signed_distance;
use crate::winding::newell;

/// Texture size assumed for faces whose material has no image.
pub const NOMINAL_SIZE: f64 = 64.0;

const AXIS_EPS: f64 = 1e-9;
const ON_PLANE_EPS: f64 = 1e-3;

/// The affine map from world positions to texel coordinates: `texel(p) = (p·gu, p·gv) + offset`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TexMap {
    pub gu: DVec3,
    pub gv: DVec3,
    pub offset: DVec2,
}

impl TexMap {
    pub fn texel(&self, p: DVec3) -> DVec2 {
        DVec2::new(p.dot(self.gu), p.dot(self.gv)) + self.offset
    }
}

/// Quake-style projection axes picked from the dominant axis of `normal`, and that axis signed to face the normal.
/// V points down the wall so images appear upright.
pub fn paraxial_axes(normal: DVec3) -> (UvAxes, DVec3) {
    let a = normal.abs();
    if a.z >= a.x && a.z >= a.y {
        (
            UvAxes {
                u: DVec3::X,
                v: -DVec3::Y,
            },
            DVec3::Z * normal.z.signum_or_one(),
        )
    } else if a.x >= a.y {
        (
            UvAxes {
                u: DVec3::Y,
                v: -DVec3::Z,
            },
            DVec3::X * normal.x.signum_or_one(),
        )
    } else {
        (
            UvAxes {
                u: DVec3::X,
                v: -DVec3::Z,
            },
            DVec3::Y * normal.y.signum_or_one(),
        )
    }
}

trait SignumOrOne {
    fn signum_or_one(self) -> f64;
}

impl SignumOrOne for f64 {
    fn signum_or_one(self) -> f64 {
        if self < 0.0 { -1.0 } else { 1.0 }
    }
}

/// Axes lying in the face's plane: U runs to the viewer's right looking at the face from outside, V runs down.
/// Horizontal faces use the world X and -Y.
pub fn face_aligned_axes(normal: DVec3) -> UvAxes {
    let right = DVec3::Z.cross(normal);
    if right.length() < 1e-6 {
        return UvAxes {
            u: DVec3::X,
            v: -DVec3::Y,
        };
    }
    let u = right.normalize();
    let up = normal.cross(u).normalize();
    UvAxes { u, v: -up }
}

pub fn texel_map(attrs: &FaceAttrs, normal: DVec3) -> TexMap {
    let (axes, spin) = match attrs.axes {
        Some(a) => (a, normal),
        None => paraxial_axes(normal),
    };
    let q = DQuat::from_axis_angle(spin.normalize_or(DVec3::Z), attrs.rotation.to_radians());
    TexMap {
        gu: (q * axes.u) / attrs.scale.x,
        gv: (q * axes.v) / attrs.scale.y,
        offset: attrs.offset,
    }
}

/// The unit axes in use after rotation, as world vectors.
pub fn effective_axes(attrs: &FaceAttrs, normal: DVec3) -> UvAxes {
    let m = texel_map(attrs, normal);
    UvAxes {
        u: m.gu * attrs.scale.x,
        v: m.gv * attrs.scale.y,
    }
}

pub fn project(attrs: &FaceAttrs, normal: DVec3, p: DVec3) -> DVec2 {
    texel_map(attrs, normal).texel(p)
}

fn tidy(x: f64) -> f64 {
    let r = (x / 1e-6).round() * 1e-6;
    r + 0.0
}

fn tidy_axis(v: DVec3) -> DVec3 {
    let r = |x: f64| (x / 1e-9).round() * 1e-9 + 0.0;
    DVec3::new(r(v.x), r(v.y), r(v.z))
}

/// Attributes that reproduce `map` on `plane`, with explicit in-plane axes.
/// `None` when the map collapses along an axis.
pub fn attrs_from_map(template: &FaceAttrs, map: &TexMap, plane: &Plane) -> Option<FaceAttrs> {
    let n = plane.normal;
    let inplane = |g: DVec3| g - n * n.dot(g);
    let gu = inplane(map.gu);
    let gv = inplane(map.gv);
    let (lu, lv) = (gu.length(), gv.length());
    if lu < AXIS_EPS || lv < AXIS_EPS {
        return None;
    }
    let offset = DVec2::new(
        map.offset.x + plane.dist * n.dot(map.gu),
        map.offset.y + plane.dist * n.dot(map.gv),
    );
    Some(FaceAttrs {
        material: template.material.clone(),
        offset: DVec2::new(tidy(offset.x), tidy(offset.y)),
        scale: DVec2::new(1.0 / lu, 1.0 / lv),
        rotation: 0.0,
        axes: Some(UvAxes {
            u: tidy_axis(gu / lu),
            v: tidy_axis(gv / lv),
        }),
    })
}

/// Like `attrs_from_map`, but keeps the template's axes, scale and rotation when only the offset needs to change.
pub fn attrs_preserving_form(
    template: &FaceAttrs,
    map: &TexMap,
    plane: &Plane,
) -> Option<FaceAttrs> {
    let n = plane.normal;
    let kept = texel_map(template, n);
    let inplane = |g: DVec3| g - n * n.dot(g);
    let same = |a: DVec3, b: DVec3| (inplane(a) - inplane(b)).length() <= 1e-7 * (1.0 + b.length());
    if same(kept.gu, map.gu) && same(kept.gv, map.gv) {
        let offset = DVec2::new(
            map.offset.x + plane.dist * n.dot(map.gu) - plane.dist * n.dot(kept.gu),
            map.offset.y + plane.dist * n.dot(map.gv) - plane.dist * n.dot(kept.gv),
        );
        let mut out = template.clone();
        out.offset = DVec2::new(tidy(offset.x), tidy(offset.y));
        return Some(out);
    }
    attrs_from_map(template, map, plane)
}

/// The map that sends each of three world points to its texel, as long as the points span a triangle.
pub fn fit_map(samples: [(DVec3, DVec2); 3]) -> Option<TexMap> {
    let [(p0, t0), (p1, t1), (p2, t2)] = samples;
    let (e1, e2) = (p1 - p0, p2 - p0);
    let (a, b, c) = (e1.dot(e1), e1.dot(e2), e2.dot(e2));
    let det = a * c - b * b;
    if det <= 1e-9 * a * c || det <= 1e-12 {
        return None;
    }
    let solve = |d1: f64, d2: f64| {
        let x = (d1 * c - d2 * b) / det;
        let y = (d2 * a - d1 * b) / det;
        e1 * x + e2 * y
    };
    let gu = solve(t1.x - t0.x, t2.x - t0.x);
    let gv = solve(t1.y - t0.y, t2.y - t0.y);
    Some(TexMap {
        gu,
        gv,
        offset: DVec2::new(t0.x - gu.dot(p0), t0.y - gv.dot(p0)),
    })
}

fn triangle_area(a: DVec3, b: DVec3, c: DVec3) -> f64 {
    (b - a).cross(c - a).length()
}

/// Re-fits `attrs` after a face's corners moved from `old` to `new` positions so that each corner keeps its texel.
/// `old_normal` is the face normal before the move and `new_plane` its plane after.
/// Returns `None` when the attributes should stay as they are: nothing moved, or the corners do not fix a mapping.
pub fn lock_corners(
    attrs: &FaceAttrs,
    old_normal: DVec3,
    new_plane: &Plane,
    pairs: &[(DVec3, DVec3)],
) -> Option<FaceAttrs> {
    let on_plane: Vec<(DVec3, DVec3)> = pairs
        .iter()
        .copied()
        .filter(|(_, new)| signed_distance(new_plane, *new).abs() <= ON_PLANE_EPS)
        .collect();
    let facing_same_way = old_normal.dot(new_plane.normal) > 1.0 - 1e-9;
    if facing_same_way && on_plane.iter().all(|(o, n)| (*o - *n).length() < 1e-9) {
        return None;
    }
    let still: Vec<(DVec3, DVec3)> = on_plane
        .iter()
        .copied()
        .filter(|(o, n)| (*o - *n).length() < 1e-9)
        .collect();
    let pool = if best_triple(&still).is_some() {
        still
    } else {
        on_plane
    };
    let (i, j, k) = best_triple(&pool)?;
    let old_map = texel_map(attrs, old_normal);
    let sample = |idx: usize| (pool[idx].1, old_map.texel(pool[idx].0));
    let map = fit_map([sample(i), sample(j), sample(k)])?;
    attrs_preserving_form(attrs, &map, new_plane)
}

fn best_triple(pairs: &[(DVec3, DVec3)]) -> Option<(usize, usize, usize)> {
    let mut best: Option<(usize, usize, usize, f64)> = None;
    for i in 0..pairs.len() {
        for j in i + 1..pairs.len() {
            for k in j + 1..pairs.len() {
                let area = triangle_area(pairs[i].1, pairs[j].1, pairs[k].1);
                if area > 1e-6 && best.is_none_or(|b| area > b.3) {
                    best = Some((i, j, k, area));
                }
            }
        }
    }
    best.map(|(i, j, k, _)| (i, j, k))
}

fn loop_plane(points: &[DVec3]) -> Option<Plane> {
    if points.len() < 3 {
        return None;
    }
    let n = newell(points);
    let len = n.length();
    if len < 1e-9 {
        return None;
    }
    let normal = n / len;
    let centroid = points.iter().copied().sum::<DVec3>() / points.len() as f64;
    Some(Plane {
        normal,
        dist: normal.dot(centroid),
    })
}

/// Re-fits every face of `new` after its vertices moved away from where `old` had them.
/// Both meshes must have the same faces and vertex ids.
pub fn lock_mesh_faces(old: &Mesh, new: &mut Mesh) {
    for face in &mut new.faces {
        let Some(before) = old.faces.iter().find(|f| f.id == face.id) else {
            continue;
        };
        let old_pts: Option<Vec<DVec3>> = before
            .verts
            .iter()
            .map(|&v| old.vertex(v).map(|x| x.pos))
            .collect();
        let new_pts: Option<Vec<DVec3>> = face
            .verts
            .iter()
            .map(|&v| new.vertices.iter().find(|x| x.id == v).map(|x| x.pos))
            .collect();
        let (Some(old_pts), Some(new_pts)) = (old_pts, new_pts) else {
            continue;
        };
        let (Some(before_plane), Some(after_plane)) = (loop_plane(&old_pts), loop_plane(&new_pts))
        else {
            continue;
        };
        let pairs: Vec<(DVec3, DVec3)> = before
            .verts
            .iter()
            .filter_map(|&v| {
                let from = old.vertex(v)?.pos;
                let to = new.vertices.iter().find(|x| x.id == v)?.pos;
                Some((from, to))
            })
            .collect();
        if let Some(attrs) = lock_corners(&face.attrs, before_plane.normal, &after_plane, &pairs) {
            face.attrs = attrs;
        }
    }
}

/// Texel bounds of a polygon's corners under `attrs`.
pub fn texel_bounds(attrs: &FaceAttrs, normal: DVec3, corners: &[DVec3]) -> Option<(DVec2, DVec2)> {
    let map = texel_map(attrs, normal);
    corners.iter().map(|&p| map.texel(p)).fold(None, |acc, t| {
        Some(match acc {
            None => (t, t),
            Some((lo, hi)) => (lo.min(t), hi.max(t)),
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plane::plane_from_points;

    fn floor() -> Plane {
        Plane {
            normal: DVec3::Z,
            dist: 0.0,
        }
    }

    #[test]
    fn paraxial_floor_projects_xy() {
        let a = FaceAttrs::default();
        let t = project(&a, DVec3::Z, DVec3::new(10.0, 20.0, 0.0));
        assert!((t - DVec2::new(10.0, -20.0)).length() < 1e-12);
    }

    #[test]
    fn scale_offset_rotation() {
        let a = FaceAttrs {
            offset: DVec2::new(3.0, 0.0),
            scale: DVec2::new(2.0, 2.0),
            ..FaceAttrs::default()
        };
        let t = project(&a, DVec3::Z, DVec3::new(10.0, 0.0, 0.0));
        assert!((t - DVec2::new(8.0, 0.0)).length() < 1e-12);
        let rotated = FaceAttrs {
            rotation: 90.0,
            ..FaceAttrs::default()
        };
        let t = project(&rotated, DVec3::Z, DVec3::new(10.0, 0.0, 0.0));
        assert!(t.x.abs() < 1e-9, "{t}");
    }

    #[test]
    fn fit_recovers_a_map() {
        let attrs = FaceAttrs {
            offset: DVec2::new(5.0, -3.0),
            scale: DVec2::new(0.5, 4.0),
            rotation: 30.0,
            ..FaceAttrs::default()
        };
        let m = texel_map(&attrs, DVec3::Z);
        let pts = [
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(8.0, 1.0, 0.0),
            DVec3::new(2.0, 9.0, 0.0),
        ];
        let fit = fit_map(pts.map(|p| (p, m.texel(p)))).unwrap();
        let probe = DVec3::new(-4.0, 7.0, 0.0);
        assert!((fit.texel(probe) - m.texel(probe)).length() < 1e-9);
    }

    #[test]
    fn collinear_samples_do_not_fit() {
        let s = |x: f64| (DVec3::new(x, 0.0, 0.0), DVec2::ZERO);
        assert!(fit_map([s(0.0), s(1.0), s(2.0)]).is_none());
    }

    #[test]
    fn translation_only_changes_the_offset() {
        let attrs = FaceAttrs::default();
        let pairs: Vec<_> = [
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(8.0, 0.0, 0.0),
            DVec3::new(0.0, 8.0, 0.0),
        ]
        .iter()
        .map(|&p| (p, p + DVec3::new(16.0, 0.0, 0.0)))
        .collect();
        let out = lock_corners(&attrs, DVec3::Z, &floor(), &pairs).unwrap();
        assert!(out.axes.is_none());
        assert_eq!(out.scale, DVec2::ONE);
        assert!((out.offset - DVec2::new(-16.0, 0.0)).length() < 1e-9);
    }

    #[test]
    fn nothing_moved_keeps_attrs() {
        let p = [DVec3::ZERO, DVec3::X, DVec3::Y];
        let pairs: Vec<_> = p.iter().map(|&p| (p, p)).collect();
        assert!(lock_corners(&FaceAttrs::default(), DVec3::Z, &floor(), &pairs).is_none());
    }

    #[test]
    fn quarter_turn_keeps_texels() {
        let attrs = FaceAttrs {
            offset: DVec2::new(1.0, 2.0),
            scale: DVec2::new(2.0, 2.0),
            ..FaceAttrs::default()
        };
        let rot = |p: DVec3| DVec3::new(-p.y, p.x, p.z);
        let corners = [
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(16.0, 0.0, 0.0),
            DVec3::new(0.0, 8.0, 0.0),
        ];
        let pairs: Vec<_> = corners.iter().map(|&p| (p, rot(p))).collect();
        let out = lock_corners(&attrs, DVec3::Z, &floor(), &pairs).unwrap();
        for p in corners {
            let before = project(&attrs, DVec3::Z, p);
            let after = project(&out, DVec3::Z, rot(p));
            assert!((before - after).length() < 1e-6);
        }
    }

    #[test]
    fn face_aligned_axes_lie_in_the_plane() {
        let p = plane_from_points([
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(0.0, 8.0, 8.0),
            DVec3::new(8.0, 0.0, 0.0),
        ])
        .unwrap();
        let axes = face_aligned_axes(p.normal);
        assert!(axes.u.dot(p.normal).abs() < 1e-9);
        assert!(axes.v.dot(p.normal).abs() < 1e-9);
    }
}
