use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const MAX_DEPTH: usize = 5;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Genome {
    pub id: Uuid,
    pub parent_ids: Vec<Uuid>,
    pub generation: u32,
    pub tree: CsgNode,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CsgNode {
    Primitive(Primitive),
    BoolOp {
        op: BoolOpType,
        left: Box<CsgNode>,
        right: Box<CsgNode>,
    },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub enum BoolOpType {
    Union,
    Intersection,
    Subtraction,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Primitive {
    Circle {
        center: Point,
        radius: f64,
    },
    Rectangle {
        center: Point,
        width: f64,
        height: f64,
        rotation: f64,
    },
    RegularPolygon {
        center: Point,
        radius: f64,
        sides: u8,
        rotation: f64,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

// Tree traversal utilities using preorder indexing

impl CsgNode {
    pub fn depth(&self) -> usize {
        match self {
            CsgNode::Primitive(_) => 1,
            CsgNode::BoolOp { left, right, .. } => {
                1 + left.depth().max(right.depth())
            }
        }
    }

    pub fn node_count(&self) -> usize {
        match self {
            CsgNode::Primitive(_) => 1,
            CsgNode::BoolOp { left, right, .. } => {
                1 + left.node_count() + right.node_count()
            }
        }
    }

    /// Get a reference to the node at the given preorder index.
    pub fn node_at(&self, index: usize) -> Option<&CsgNode> {
        if index == 0 {
            return Some(self);
        }
        match self {
            CsgNode::Primitive(_) => None,
            CsgNode::BoolOp { left, right, .. } => {
                let left_count = left.node_count();
                if index - 1 < left_count {
                    left.node_at(index - 1)
                } else {
                    right.node_at(index - 1 - left_count)
                }
            }
        }
    }

    /// Return a new tree with the node at the given preorder index replaced.
    pub fn replace_at(&self, index: usize, replacement: CsgNode) -> CsgNode {
        if index == 0 {
            return replacement;
        }
        match self {
            CsgNode::Primitive(_) => self.clone(),
            CsgNode::BoolOp { op, left, right } => {
                let left_count = left.node_count();
                if index - 1 < left_count {
                    CsgNode::BoolOp {
                        op: *op,
                        left: Box::new(left.replace_at(index - 1, replacement)),
                        right: right.clone(),
                    }
                } else {
                    CsgNode::BoolOp {
                        op: *op,
                        left: left.clone(),
                        right: Box::new(right.replace_at(index - 1 - left_count, replacement)),
                    }
                }
            }
        }
    }
}
