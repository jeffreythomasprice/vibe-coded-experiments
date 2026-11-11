use editor_core::{Brush, BrushFace, FaceAttrs, FaceId};
use glam::DVec3;

/// `p0, p1, p2` run counter-clockwise seen from outside; stored points use the clockwise convention.
fn face(id: u32, p0: DVec3, p1: DVec3, p2: DVec3) -> BrushFace {
    BrushFace {
        id: FaceId(id),
        points: [p0, p2, p1],
        attrs: FaceAttrs::default(),
    }
}

pub fn cuboid(min: DVec3, max: DVec3) -> Brush {
    let (x0, y0, z0) = (min.x, min.y, min.z);
    let (x1, y1, z1) = (max.x, max.y, max.z);
    let v = DVec3::new;
    Brush {
        faces: vec![
            face(1, v(x0, y0, z1), v(x1, y0, z1), v(x1, y1, z1)),
            face(2, v(x0, y0, z0), v(x0, y1, z0), v(x1, y1, z0)),
            face(3, v(x1, y0, z0), v(x1, y1, z0), v(x1, y1, z1)),
            face(4, v(x0, y0, z0), v(x0, y0, z1), v(x0, y1, z1)),
            face(5, v(x0, y1, z0), v(x0, y1, z1), v(x1, y1, z1)),
            face(6, v(x0, y0, z0), v(x1, y0, z0), v(x1, y0, z1)),
        ],
    }
}

pub fn prism_vertices(center: DVec3, radius: f64, sides: usize, z: f64) -> Vec<DVec3> {
    (0..sides)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / sides as f64;
            DVec3::new(center.x + radius * a.cos(), center.y + radius * a.sin(), z)
        })
        .collect()
}

pub fn prism(center: DVec3, radius: f64, height: f64, sides: usize) -> Brush {
    assert!(sides >= 3);
    let bottom = prism_vertices(center, radius, sides, center.z - height / 2.0);
    let top = prism_vertices(center, radius, sides, center.z + height / 2.0);
    let mut faces = vec![
        face(1, top[0], top[1], top[2]),
        face(2, bottom[0], bottom[2], bottom[1]),
    ];
    for i in 0..sides {
        let j = (i + 1) % sides;
        faces.push(face(3 + i as u32, bottom[i], bottom[j], top[j]));
    }
    Brush { faces }
}

/// A ramp in the box `min..max` that rises from zero height at the low end to the full height at the
/// end `toward` points to (an axis-aligned unit vector in the xy plane).
pub fn wedge(min: DVec3, max: DVec3, toward: DVec3) -> Brush {
    let (mut lo, mut hi) = (min, max);
    if toward.x < 0.0 {
        std::mem::swap(&mut lo.x, &mut hi.x);
    }
    if toward.y < 0.0 {
        std::mem::swap(&mut lo.y, &mut hi.y);
    }
    let along_x = toward.x != 0.0;
    let v = DVec3::new;
    let (a, b, c, d, e, f) = if along_x {
        (
            v(lo.x, min.y, min.z),
            v(hi.x, min.y, min.z),
            v(hi.x, max.y, min.z),
            v(lo.x, max.y, min.z),
            v(hi.x, min.y, max.z),
            v(hi.x, max.y, max.z),
        )
    } else {
        (
            v(min.x, lo.y, min.z),
            v(min.x, hi.y, min.z),
            v(max.x, hi.y, min.z),
            v(max.x, lo.y, min.z),
            v(min.x, hi.y, max.z),
            v(max.x, hi.y, max.z),
        )
    };
    let centroid = [a, b, c, d, e, f].into_iter().sum::<DVec3>() / 6.0;
    let outward = |id: u32, p: [DVec3; 3]| {
        let normal = (p[1] - p[0]).cross(p[2] - p[0]);
        if normal.dot(p[0] - centroid) >= 0.0 {
            face(id, p[0], p[1], p[2])
        } else {
            face(id, p[0], p[2], p[1])
        }
    };
    Brush {
        faces: vec![
            outward(1, [a, b, c]),
            outward(2, [b, c, f]),
            outward(3, [a, b, e]),
            outward(4, [d, c, f]),
            outward(5, [a, d, f]),
        ],
    }
}
