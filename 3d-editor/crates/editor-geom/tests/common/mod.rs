#![allow(dead_code)]

use editor_core::{Brush, FaceAttrs, Mesh};
use editor_geom::mesh::{MeshIssue, validate};
use editor_geom::shapes::cuboid;
use editor_geom::volume::mesh_volume;
use editor_geom::{brush_geometry, brush_to_mesh, clip_brush};
use glam::DVec3;
use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed, TestRunner};
use std::cell::Cell;

pub fn build_brush(min: [i32; 3], size: [i32; 3], cuts: &[([i32; 9], bool)]) -> Brush {
    let lo = DVec3::new(min[0] as f64, min[1] as f64, min[2] as f64);
    let hi = lo + DVec3::new(size[0] as f64, size[1] as f64, size[2] as f64);
    let mut brush = cuboid(lo, hi);
    for (c, keep_back) in cuts {
        let p = |i: usize| DVec3::new(c[i] as f64, c[i + 1] as f64, c[i + 2] as f64);
        let Ok(r) = clip_brush(&brush, [p(0), p(3), p(6)], FaceAttrs::default()) else {
            continue;
        };
        let piece = if *keep_back { r.back } else { r.front };
        if let Some(b) = piece {
            brush = b;
        }
    }
    brush
}

pub fn brush_strategy() -> impl Strategy<Value = Brush> {
    (
        prop::array::uniform3(-8i32..8),
        prop::array::uniform3(2i32..16),
        prop::collection::vec((prop::array::uniform9(-12i32..24), any::<bool>()), 0..4),
    )
        .prop_map(|(min, size, cuts)| build_brush(min, size, &cuts))
}

pub fn plane_points_strategy() -> impl Strategy<Value = [DVec3; 3]> {
    prop::array::uniform9(-12i32..24).prop_map(|c| {
        let p = |i: usize| DVec3::new(c[i] as f64, c[i + 1] as f64, c[i + 2] as f64);
        [p(0), p(3), p(6)]
    })
}

pub fn brush_volume(b: &Brush) -> f64 {
    brush_geometry(b).unwrap().volume()
}

pub fn brush_mesh(b: &Brush) -> Mesh {
    brush_to_mesh(b).unwrap()
}

pub fn surface_area(b: &Brush) -> f64 {
    let geo = brush_geometry(b).unwrap();
    geo.faces
        .iter()
        .map(|f| editor_geom::winding::area(&geo.face_positions(f)))
        .sum()
}

pub fn topology_issues(mesh: &Mesh) -> Vec<MeshIssue> {
    validate(mesh)
        .into_iter()
        .filter(|i| !matches!(i, MeshIssue::NonPlanarFace { .. }))
        .collect()
}

pub fn assert_clean(mesh: &Mesh) {
    let issues = validate(mesh);
    assert!(issues.is_empty(), "issues: {issues:?}");
}

pub fn assert_volume(mesh: &Mesh, expected: f64, tol: f64) {
    let v = mesh_volume(mesh);
    assert!(
        (v - expected).abs() <= tol,
        "volume {v} expected {expected} (tol {tol})"
    );
}

/// Like `topology_issues`, but a pinched edge with as many forward as backward uses is still watertight.
pub fn watertight_issues(mesh: &Mesh) -> Vec<MeshIssue> {
    topology_issues(mesh)
        .into_iter()
        .filter(|i| !matches!(i, MeshIssue::NonManifoldEdge { forward, backward, .. } if forward == backward))
        .collect()
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Runs `test` over `cases` inputs under each of several fixed seeds.
/// `EDITOR_GEOM_SEEDS` sets how many seeds (default 8); `EDITOR_GEOM_SEED_BASE` shifts the range.
/// A failure panics with the seed so it can be replayed exactly.
pub fn check<S: Strategy>(
    strategy: S,
    cases: u32,
    test: impl Fn(S::Value) -> Result<(), TestCaseError>,
) {
    let seeds = env_u64("EDITOR_GEOM_SEEDS", 8);
    let base = env_u64("EDITOR_GEOM_SEED_BASE", 0);
    for seed in base..base + seeds {
        let mut runner = TestRunner::new(Config {
            cases,
            rng_seed: RngSeed::Fixed(seed),
            failure_persistence: None,
            ..Config::default()
        });
        if let Err(e) = runner.run(&strategy, &test) {
            panic!("failed under EDITOR_GEOM_SEED_BASE={seed} EDITOR_GEOM_SEEDS=1: {e}");
        }
    }
}

/// Counts operations that were allowed to fail with a typed error, so the failure rate stays bounded.
#[derive(Default)]
pub struct Tally {
    attempts: Cell<u32>,
    rejected: Cell<u32>,
}

impl Tally {
    pub fn record<T, E>(&self, r: Result<T, E>) -> Option<T> {
        self.attempts.set(self.attempts.get() + 1);
        match r {
            Ok(v) => Some(v),
            Err(_) => {
                self.rejected.set(self.rejected.get() + 1);
                None
            }
        }
    }

    pub fn assert_rate_below(&self, max_fraction: f64) {
        let (n, bad) = (self.attempts.get(), self.rejected.get());
        assert!(
            bad as f64 <= max_fraction * n as f64,
            "{bad} of {n} operations returned an error (limit {max_fraction})"
        );
    }
}
