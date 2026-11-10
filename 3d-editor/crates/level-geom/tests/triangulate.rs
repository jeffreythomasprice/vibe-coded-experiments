use glam::DVec3;
use level_geom::mesh::triangulate;
use level_geom::winding::{area, newell};
mod common;

use common::check;
use proptest::prelude::*;

#[test]
fn star_polygon_area_preserved() {
    let strategy = (
        prop::collection::vec((0.6f64..1.0, 1.0f64..10.0), 3..14),
        0usize..3,
    );
    check(strategy, 256, |(raw, tilt)| {
        let total: f64 = raw.iter().map(|(g, _)| g).sum();
        let mut angle = 0.0_f64;
        let flat: Vec<DVec3> = raw
            .iter()
            .map(|(g, r)| {
                let p = DVec3::new(r * angle.cos(), r * angle.sin(), 3.0);
                angle += g / total * std::f64::consts::TAU;
                p
            })
            .collect();
        let poly: Vec<DVec3> = flat
            .iter()
            .map(|p| match tilt {
                0 => *p,
                1 => DVec3::new(p.z, p.x, p.y),
                _ => DVec3::new(p.y, p.z, p.x),
            })
            .collect();
        let expected = area(&poly);
        let tris = triangulate(&poly);
        prop_assert_eq!(tris.len(), poly.len() - 2);
        let n = newell(&poly);
        let mut sum = 0.0;
        for t in &tris {
            let tri = [poly[t[0]], poly[t[1]], poly[t[2]]];
            let tn = newell(&tri);
            prop_assert!(tn.dot(n) >= -1e-9, "triangle flipped");
            sum += area(&tri);
        }
        prop_assert!(
            (sum - expected).abs() < 1e-6 * expected.max(1.0),
            "{sum} vs {expected}"
        );
        Ok(())
    });
}
