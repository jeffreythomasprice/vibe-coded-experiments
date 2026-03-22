use super::types::*;

/// Compute a normalized distance in [0, 1] between two genomes.
pub fn genome_distance(a: &Genome, b: &Genome) -> f64 {
    tree_distance(&a.tree, &b.tree).clamp(0.0, 1.0)
}

fn tree_distance(a: &CsgNode, b: &CsgNode) -> f64 {
    match (a, b) {
        (CsgNode::Primitive(pa), CsgNode::Primitive(pb)) => primitive_distance(pa, pb),
        (
            CsgNode::BoolOp {
                op: op_a,
                left: la,
                right: ra,
            },
            CsgNode::BoolOp {
                op: op_b,
                left: lb,
                right: rb,
            },
        ) => {
            let op_d = if op_a == op_b { 0.0 } else { 0.3 };
            let left_d = tree_distance(la, lb);
            let right_d = tree_distance(ra, rb);
            op_d * 0.2 + (left_d + right_d) * 0.4
        }
        // Mismatched structure
        _ => 0.8,
    }
}

fn primitive_distance(a: &Primitive, b: &Primitive) -> f64 {
    let (center_a, size_a, rot_a) = primitive_params(a);
    let (center_b, size_b, rot_b) = primitive_params(b);

    let type_d = if primitive_type_matches(a, b) {
        0.0
    } else {
        0.3
    };
    let center_d = point_distance(&center_a, &center_b);
    let size_d = (size_a - size_b).abs() / 0.45;
    let rot_d = angle_distance(rot_a, rot_b);

    type_d * 0.2 + center_d * 0.4 + size_d * 0.2 + rot_d * 0.2
}

fn primitive_params(p: &Primitive) -> (Point, f64, f64) {
    match p {
        Primitive::Circle { center, radius } => (center.clone(), *radius, 0.0),
        Primitive::Rectangle {
            center,
            width,
            height,
            rotation,
        } => (center.clone(), (*width + *height) * 0.5, *rotation),
        Primitive::RegularPolygon {
            center,
            radius,
            rotation,
            ..
        } => (center.clone(), *radius, *rotation),
    }
}

fn primitive_type_matches(a: &Primitive, b: &Primitive) -> bool {
    matches!(
        (a, b),
        (Primitive::Circle { .. }, Primitive::Circle { .. })
            | (Primitive::Rectangle { .. }, Primitive::Rectangle { .. })
            | (
                Primitive::RegularPolygon { .. },
                Primitive::RegularPolygon { .. }
            )
    )
}

fn point_distance(a: &Point, b: &Point) -> f64 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    (dx * dx + dy * dy).sqrt() / std::f64::consts::SQRT_2
}

fn angle_distance(a: f64, b: f64) -> f64 {
    let d = (a - b).abs() % (2.0 * std::f64::consts::PI);
    let d = if d > std::f64::consts::PI {
        2.0 * std::f64::consts::PI - d
    } else {
        d
    };
    d / std::f64::consts::PI
}
