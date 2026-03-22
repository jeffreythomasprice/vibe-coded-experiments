use rand::Rng;

use super::random::{random_primitive, random_tree};
use super::types::*;

const PRIMITIVE_MUTATION_RATE: f64 = 0.30;
const OP_SWAP_RATE: f64 = 0.10;
const GROW_RATE: f64 = 0.05;
const PRUNE_RATE: f64 = 0.05;
const SUBTREE_REPLACE_RATE: f64 = 0.02;
const MUTATION_SIGMA: f64 = 0.1;

pub fn mutate(genome: &mut Genome) {
    let mut rng = rand::thread_rng();
    genome.tree = mutate_tree(&genome.tree, &mut rng);
}

fn mutate_tree(node: &CsgNode, rng: &mut impl Rng) -> CsgNode {
    // Structural mutations first (applied to the whole node)

    // Grow: replace a leaf with a boolean op
    if matches!(node, CsgNode::Primitive(_)) && rng.gen_bool(GROW_RATE) {
        let current_depth = node.depth();
        if current_depth < MAX_DEPTH {
            return CsgNode::BoolOp {
                op: random_bool_op(rng),
                left: Box::new(node.clone()),
                right: Box::new(CsgNode::Primitive(random_primitive(rng))),
            };
        }
    }

    // Prune: replace a bool op with one of its children
    if let CsgNode::BoolOp { left, right, .. } = node {
        if rng.gen_bool(PRUNE_RATE) {
            return if rng.gen_bool(0.5) {
                *left.clone()
            } else {
                *right.clone()
            };
        }
    }

    // Subtree replacement
    if rng.gen_bool(SUBTREE_REPLACE_RATE) {
        return random_tree(rng, 2.min(MAX_DEPTH - 1));
    }

    match node {
        CsgNode::Primitive(p) => {
            CsgNode::Primitive(mutate_primitive(p, rng))
        }
        CsgNode::BoolOp { op, left, right } => {
            let new_op = if rng.gen_bool(OP_SWAP_RATE) {
                random_bool_op(rng)
            } else {
                *op
            };
            CsgNode::BoolOp {
                op: new_op,
                left: Box::new(mutate_tree(left, rng)),
                right: Box::new(mutate_tree(right, rng)),
            }
        }
    }
}

fn mutate_primitive(p: &Primitive, rng: &mut impl Rng) -> Primitive {
    if !rng.gen_bool(PRIMITIVE_MUTATION_RATE) {
        return p.clone();
    }

    match p {
        Primitive::Circle { center, radius } => Primitive::Circle {
            center: mutate_point(center, rng),
            radius: (*radius + gaussian(rng, MUTATION_SIGMA * 0.5)).clamp(0.02, 0.45),
        },
        Primitive::Rectangle {
            center,
            width,
            height,
            rotation,
        } => Primitive::Rectangle {
            center: mutate_point(center, rng),
            width: (*width + gaussian(rng, MUTATION_SIGMA * 0.5)).clamp(0.02, 0.45),
            height: (*height + gaussian(rng, MUTATION_SIGMA * 0.5)).clamp(0.02, 0.45),
            rotation: rotation + gaussian(rng, MUTATION_SIGMA),
        },
        Primitive::RegularPolygon {
            center,
            radius,
            sides,
            rotation,
        } => {
            let new_sides = if rng.gen_bool(0.1) {
                rng.gen_range(3..=8)
            } else {
                *sides
            };
            Primitive::RegularPolygon {
                center: mutate_point(center, rng),
                radius: (*radius + gaussian(rng, MUTATION_SIGMA * 0.5)).clamp(0.02, 0.45),
                sides: new_sides,
                rotation: rotation + gaussian(rng, MUTATION_SIGMA),
            }
        }
    }
}

fn mutate_point(p: &Point, rng: &mut impl Rng) -> Point {
    Point {
        x: (p.x + gaussian(rng, MUTATION_SIGMA)).clamp(0.05, 0.95),
        y: (p.y + gaussian(rng, MUTATION_SIGMA)).clamp(0.05, 0.95),
    }
}

fn random_bool_op(rng: &mut impl Rng) -> BoolOpType {
    match rng.gen_range(0..3) {
        0 => BoolOpType::Union,
        1 => BoolOpType::Intersection,
        _ => BoolOpType::Subtraction,
    }
}

fn gaussian(rng: &mut impl Rng, sigma: f64) -> f64 {
    let u1: f64 = rng.gen();
    let u2: f64 = rng.gen();
    let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
    z * sigma
}
