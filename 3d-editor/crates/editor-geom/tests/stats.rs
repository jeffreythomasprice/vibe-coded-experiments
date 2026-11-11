mod common;

use common::*;
use editor_geom::mesh::{intersect, subtract, union};
use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::{Config, TestRunner};

#[test]
#[ignore]
fn failure_rates() {
    let mut runner = TestRunner::new(Config::default());
    let n = 10000;
    let (mut single_bad, mut chain_bad) = (0, 0);
    let s = brush_strategy();
    for _ in 0..n {
        let a = brush_mesh(&s.new_tree(&mut runner).unwrap().current());
        let b = brush_mesh(&s.new_tree(&mut runner).unwrap().current());
        let c = brush_mesh(&s.new_tree(&mut runner).unwrap().current());
        let single = [union(&a, &b), subtract(&a, &b), intersect(&a, &b)];
        if single.iter().any(|r| r.is_err()) {
            single_bad += 1;
        }
        if subtract(&a, &b).and_then(|ab| subtract(&ab, &c)).is_err() {
            chain_bad += 1;
        }
    }
    eprintln!("single-op CsgError::InvalidResult: {single_bad}/{n}; chained: {chain_bad}/{n}");
}
