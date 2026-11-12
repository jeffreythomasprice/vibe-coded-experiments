use std::collections::BTreeMap;

use editor_core::{EntityId, ObjectId, Value};
use glam::{DAffine3, DMat3, DQuat, DVec3};

#[allow(clippy::derivable_impls)]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/components.rs"));
}

pub use generated::{Interpolation, Keyframe, KeyframeAnimation, PlayerStart};

/// The document id an ECS entity was built from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Source(pub EntityId);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    pub translation: DVec3,
    pub rotation: DQuat,
}

impl Pose {
    pub fn new(translation: DVec3, rotation: DMat3) -> Self {
        Self {
            translation,
            rotation: DQuat::from_mat3(&rotation),
        }
    }

    pub fn to_affine(self) -> DAffine3 {
        DAffine3::from_rotation_translation(self.rotation, self.translation)
    }
}

/// The entity's origin and angles as authored; the implied transform component every entity has.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RestPose(pub Pose);

/// Where the entity is now; equals the rest pose until something moves it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CurrentPose(pub Pose);

/// The brushes and meshes the entity owns.
#[derive(Debug, Clone, PartialEq)]
pub struct Members(pub Vec<ObjectId>);

/// Components the simulation does not interpret, kept by name for game code.
#[derive(Debug, Clone, PartialEq)]
pub struct Dynamic(pub BTreeMap<String, Value>);

pub use crate::anim::Track;
