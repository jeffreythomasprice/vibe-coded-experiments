use rand::Rng;
use uuid::Uuid;

use super::types::*;

const MAX_INITIAL_DEPTH: usize = 2;
const MIN_POLYGON_SIDES: u8 = 3;
const MAX_POLYGON_SIDES: u8 = 8;

pub fn random_genome(generation: u32) -> Genome {
    let mut rng = rand::thread_rng();
    let tree = random_tree(&mut rng, MAX_INITIAL_DEPTH);

    Genome {
        id: Uuid::new_v4(),
        parent_ids: vec![],
        generation,
        tree,
    }
}

pub fn random_tree(rng: &mut impl Rng, max_depth: usize) -> CsgNode {
    if max_depth == 0 {
        return CsgNode::Primitive(random_primitive(rng));
    }
    // At depth 1, 50% chance of leaf vs. branch
    if max_depth == 1 && rng.gen_bool(0.5) {
        return CsgNode::Primitive(random_primitive(rng));
    }
    CsgNode::BoolOp {
        op: random_op(rng),
        left: Box::new(random_tree(rng, max_depth - 1)),
        right: Box::new(random_tree(rng, max_depth - 1)),
    }
}

pub fn random_primitive(rng: &mut impl Rng) -> Primitive {
    match rng.gen_range(0..3) {
        0 => Primitive::Circle {
            center: random_center(rng),
            radius: rng.gen_range(0.05..0.35),
        },
        1 => Primitive::Rectangle {
            center: random_center(rng),
            width: rng.gen_range(0.05..0.35),
            height: rng.gen_range(0.05..0.35),
            rotation: rng.gen_range(0.0..std::f64::consts::TAU),
        },
        _ => Primitive::RegularPolygon {
            center: random_center(rng),
            radius: rng.gen_range(0.05..0.35),
            sides: rng.gen_range(MIN_POLYGON_SIDES..=MAX_POLYGON_SIDES),
            rotation: rng.gen_range(0.0..std::f64::consts::TAU),
        },
    }
}

fn random_center(rng: &mut impl Rng) -> Point {
    Point {
        x: rng.gen_range(0.15..0.85),
        y: rng.gen_range(0.15..0.85),
    }
}

fn random_op(rng: &mut impl Rng) -> BoolOpType {
    match rng.gen_range(0..3) {
        0 => BoolOpType::Union,
        1 => BoolOpType::Intersection,
        _ => BoolOpType::Subtraction,
    }
}
