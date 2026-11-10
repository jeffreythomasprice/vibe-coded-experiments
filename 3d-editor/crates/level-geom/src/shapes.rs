use glam::DVec3;
use level_core::{Brush, BrushFace, FaceAttrs, FaceId};

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
