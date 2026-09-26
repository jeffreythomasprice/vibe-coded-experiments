use glam::Vec2;

use super::aabb::Aabb;
use super::primitive::{Primitive, Sample, Similarity};

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct NodeId(u32);

enum Node {
    Leaf(Primitive),
    Union(Vec<NodeId>),
    Intersect(Vec<NodeId>),
    Complement(NodeId),
    Transform(NodeId, Similarity),
}

pub(crate) enum NodeRef<'a> {
    Leaf(&'a Primitive),
    Union(&'a [NodeId]),
    Intersect(&'a [NodeId]),
    Complement(NodeId),
    Transform(NodeId, &'a Similarity),
}

/// An arena of CSG nodes. Nodes are only ever appended, and a node can only
/// reference children created before it, so a single forward pass over the
/// arena is enough to compute bounds bottom-up — see `finalize`.
///
/// AABB-based pruning (skip a child once its bounding box proves it can't
/// beat the current best) is only sound at `Union` nodes: the box gives a
/// *lower* bound on distance, exactly what `min` needs to discard a
/// subtree. `Intersect` would need an *upper* bound, which a box doesn't
/// give, and a bare `Complement`'s box is unbounded (see the design plan's
/// "Normal form" note). So pruning happens only in `eval`'s `Union` arm.
pub struct Csg {
    nodes: Vec<Node>,
    bounds: Vec<Aabb>,
}

impl Csg {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            bounds: Vec::new(),
        }
    }

    fn push(&mut self, node: Node) -> NodeId {
        let id = NodeId(self.nodes.len() as u32);
        self.nodes.push(node);
        self.bounds.push(Aabb::EMPTY);
        id
    }

    pub fn leaf(&mut self, prim: Primitive) -> NodeId {
        self.push(Node::Leaf(prim))
    }

    pub fn union(&mut self, kids: Vec<NodeId>) -> NodeId {
        self.push(Node::Union(kids))
    }

    pub fn intersect(&mut self, kids: Vec<NodeId>) -> NodeId {
        self.push(Node::Intersect(kids))
    }

    pub fn complement(&mut self, child: NodeId) -> NodeId {
        self.push(Node::Complement(child))
    }

    /// `Difference(a, b) := Intersect(a, Complement(b))`, carved out via
    /// hard booleans only — smooth blending is a render-only attribute on
    /// the field bake, never part of the shared tree (see the design plan's
    /// SDF-correctness note: smooth-min isn't 1-Lipschitz, so it can
    /// overestimate distance, which is unsafe for sphere-trace collision,
    /// and `i_overlay` has no equivalent for it).
    pub fn difference(&mut self, a: NodeId, b: NodeId) -> NodeId {
        let not_b = self.complement(b);
        self.intersect(vec![a, not_b])
    }

    pub fn transform(&mut self, child: NodeId, sim: Similarity) -> NodeId {
        self.push(Node::Transform(child, sim))
    }

    pub fn bounds(&self, id: NodeId) -> Aabb {
        self.bounds[id.0 as usize]
    }

    /// Recomputes cached bounds for every node. Call after building (or
    /// changing) the tree and before `eval`/`bounds`.
    pub fn finalize(&mut self) {
        for i in 0..self.nodes.len() {
            self.bounds[i] = self.compute_bounds(NodeId(i as u32));
        }
    }

    fn compute_bounds(&self, id: NodeId) -> Aabb {
        match &self.nodes[id.0 as usize] {
            Node::Leaf(p) => p.aabb(),
            Node::Union(kids) => kids
                .iter()
                .fold(Aabb::EMPTY, |acc, &k| acc.union(self.bounds[k.0 as usize])),
            Node::Intersect(kids) => kids
                .iter()
                .fold(Aabb::INFINITE, |acc, &k| acc.intersect(self.bounds[k.0 as usize])),
            Node::Complement(_) => Aabb::INFINITE,
            Node::Transform(child, sim) => self.bounds[child.0 as usize].transform(sim),
        }
    }

    /// Read-only view of a node's structure, for algorithms (contour
    /// export, tessellation) that need to walk the tree without `eval`'s
    /// distance-field semantics. Keeps `Node` itself private.
    pub(crate) fn node(&self, id: NodeId) -> NodeRef<'_> {
        match &self.nodes[id.0 as usize] {
            Node::Leaf(p) => NodeRef::Leaf(p),
            Node::Union(kids) => NodeRef::Union(kids),
            Node::Intersect(kids) => NodeRef::Intersect(kids),
            Node::Complement(child) => NodeRef::Complement(*child),
            Node::Transform(child, sim) => NodeRef::Transform(*child, sim),
        }
    }

    pub fn eval(&self, id: NodeId, p: Vec2) -> Sample {
        match &self.nodes[id.0 as usize] {
            Node::Leaf(prim) => prim.sample(p),
            Node::Union(kids) => {
                let mut best: Option<Sample> = None;
                for &k in kids {
                    let lb = self.bounds[k.0 as usize].dist_to(p);
                    if let Some(b) = &best
                        && lb > 0.0
                        && lb >= b.d
                    {
                        continue;
                    }
                    let s = self.eval(k, p);
                    if best.as_ref().is_none_or(|b| s.d < b.d) {
                        best = Some(s);
                    }
                }
                best.expect("Union must have at least one child")
            }
            Node::Intersect(kids) => kids
                .iter()
                .map(|&k| self.eval(k, p))
                .reduce(|a, b| if a.d > b.d { a } else { b })
                .expect("Intersect must have at least one child"),
            Node::Complement(child) => {
                let s = self.eval(*child, p);
                Sample {
                    d: -s.d,
                    grad: -s.grad,
                }
            }
            Node::Transform(child, sim) => {
                let local_p = sim.to_local(p);
                let s = self.eval(*child, local_p);
                Sample {
                    d: sim.scale_dist(s.d),
                    grad: sim.rotate_dir(s.grad),
                }
            }
        }
    }
}

impl Default for Csg {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::primitive::{Circle, RoundRect};

    #[test]
    fn union_matches_min_of_children() {
        let mut csg = Csg::new();
        let a = csg.leaf(Primitive::Circle(Circle { radius: 1.0 }));
        let b_leaf = csg.leaf(Primitive::Circle(Circle { radius: 1.0 }));
        let b = csg.transform(b_leaf, Similarity::translate(Vec2::new(3.0, 0.0)));
        let root = csg.union(vec![a, b]);
        csg.finalize();

        // Near `a`, the union should equal `a` alone.
        let p = Vec2::new(0.2, 0.0);
        let expected = Primitive::Circle(Circle { radius: 1.0 }).sample(p).d;
        assert!((csg.eval(root, p).d - expected).abs() < 1e-4);

        // Between the two circles, sign must be positive (outside both).
        assert!(csg.eval(root, Vec2::new(1.5, 0.0)).d > 0.0);
    }

    #[test]
    fn difference_carves_a_hole() {
        let mut csg = Csg::new();
        let outer = csg.leaf(Primitive::RoundRect(RoundRect {
            half_extents: Vec2::new(2.0, 2.0),
            radius: 0.0,
        }));
        let hole = csg.leaf(Primitive::Circle(Circle { radius: 0.5 }));
        let root = csg.difference(outer, hole);
        csg.finalize();

        // Center is inside the hole -> outside the resulting shape.
        assert!(csg.eval(root, Vec2::ZERO).d > 0.0);
        // Near the outer edge, still inside the resulting shape.
        assert!(csg.eval(root, Vec2::new(1.9, 0.0)).d < 0.0);
    }

    /// `min`/`max` composition is exact only for unions of convex shapes;
    /// near a concave join the magnitude is a conservative *underestimate*
    /// of the true distance (see the design plan). This test locks in the
    /// half of that guarantee that matters for collision safety: the
    /// composed field never *overestimates* clearance.
    ///
    /// Ground truth here is the boundary that `contour::extract` (an
    /// entirely independent, i_overlay-based code path) computes for the
    /// same tree — not each primitive's own boundary in isolation, which
    /// would ignore that union can swallow part of a primitive's edge and
    /// that the hole adds its own boundary. Two independent methods
    /// (analytic CSG eval vs. exact polygon booleans) agreeing is exactly
    /// the cross-check the design plan calls for at the crisp/glow
    /// comparison in M5, just exercised here as a regression test.
    #[test]
    fn composition_never_overestimates_distance() {
        let mut csg = Csg::new();
        let a = csg.leaf(Primitive::RoundRect(RoundRect {
            half_extents: Vec2::new(1.0, 1.0),
            radius: 0.0,
        }));
        let b_leaf = csg.leaf(Primitive::RoundRect(RoundRect {
            half_extents: Vec2::new(1.0, 1.0),
            radius: 0.0,
        }));
        let b = csg.transform(b_leaf, Similarity::translate(Vec2::new(1.5, 0.0)));
        let union = csg.union(vec![a, b]);
        let hole = csg.leaf(Primitive::Circle(Circle { radius: 0.4 }));
        let root = csg.difference(union, hole);
        csg.finalize();

        let shapes = crate::geom::contour::extract(&csg, root, 0.001)
            .expect("overlay should succeed");

        let closest_over_shapes = |p: Vec2| -> f32 {
            let mut best = f32::INFINITY;
            for shape in &shapes {
                for contour in shape {
                    let n = contour.len();
                    for i in 0..n {
                        let a = contour[i];
                        let b = contour[(i + 1) % n];
                        let ab = b - a;
                        let t = ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
                        best = best.min((p - (a + ab * t)).length());
                    }
                }
            }
            best
        };

        for x in [-0.8, -0.3, 0.3, 0.8, 1.5, 2.0, 2.8] {
            for y in [-0.8, -0.3, 0.0, 0.3, 0.8] {
                let p = Vec2::new(x, y);
                let true_unsigned = closest_over_shapes(p);
                let composed = csg.eval(root, p).d.abs();
                // Epsilon comfortably exceeds the flattened circle's own
                // polygon-approximation error at the `tol` passed to
                // `extract` above, so it isn't masking a real discrepancy.
                assert!(
                    composed <= true_unsigned + 0.005,
                    "at {p:?}: composed |d|={composed} exceeds true unsigned distance {true_unsigned}"
                );
            }
        }
    }
}
