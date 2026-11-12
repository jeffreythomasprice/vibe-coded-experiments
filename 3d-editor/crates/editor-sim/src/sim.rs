use std::collections::{BTreeMap, HashMap, HashSet};

use editor_core::{Document, EntityId, ObjectId, Value};
use editor_geom::euler::orientation;
use glam::DAffine3;

use crate::components::{
    CurrentPose, Dynamic, KeyframeAnimation, Members, PlayerStart, Pose, RestPose, Source, Track,
};
use crate::world::{Entity, Schedule, World};

#[derive(Debug, Clone, Copy, Default)]
pub struct Time {
    pub elapsed: f64,
    pub dt: f64,
}

/// Brushes and meshes that belong to no entity: they never move.
#[derive(Debug, Clone, Default)]
pub struct StaticGeometry {
    pub objects: Vec<ObjectId>,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum LoadWarning {
    #[error("{entity}: component `{component}` was skipped: {error}")]
    BadComponent {
        entity: EntityId,
        component: String,
        error: String,
    },
}

pub struct Sim {
    world: World,
    schedule: Schedule,
    by_source: BTreeMap<EntityId, Entity>,
    excluded: HashSet<ObjectId>,
}

fn animate(world: &mut World) {
    let elapsed = world.resource::<Time>().map_or(0.0, |t| t.elapsed);
    let updates: Vec<(Entity, Pose)> = world
        .iter::<Track>()
        .filter_map(|(e, track)| {
            let rest = world.get::<RestPose>(e)?.0;
            let sample = track.sample(elapsed);
            let delta = orientation(sample.angles);
            let rotation = delta * glam::DMat3::from_quat(rest.rotation);
            Some((e, Pose::new(rest.translation + sample.offset, rotation)))
        })
        .collect();
    for (e, pose) in updates {
        world.insert(e, CurrentPose(pose));
    }
}

impl Sim {
    pub fn load(doc: &Document) -> (Self, Vec<LoadWarning>) {
        let mut world = World::new();
        let mut warnings = Vec::new();
        let mut by_source = BTreeMap::new();
        let mut excluded = HashSet::new();
        let omitted: HashSet<_> = doc
            .layers
            .values()
            .filter(|l| l.omit_from_export)
            .map(|l| l.id)
            .collect();

        let mut owners: HashMap<EntityId, Vec<ObjectId>> = HashMap::new();
        let mut static_objects = Vec::new();
        for (id, entity, layer) in doc
            .brushes
            .values()
            .map(|b| (ObjectId::from(b.id), b.entity, b.layer))
            .chain(
                doc.meshes
                    .values()
                    .map(|m| (ObjectId::from(m.id), m.entity, m.layer)),
            )
        {
            if omitted.contains(&layer) {
                excluded.insert(id);
                continue;
            }
            match entity {
                Some(e) => owners.entry(e).or_default().push(id),
                None => static_objects.push(id),
            }
        }
        static_objects.sort();

        for source in doc.entities.values() {
            if omitted.contains(&source.layer) {
                excluded.insert(source.id.into());
                continue;
            }
            let e = world.spawn();
            by_source.insert(source.id, e);
            world.insert(e, Source(source.id));
            let rest = Pose::new(source.origin, orientation(source.angles));
            world.insert(e, RestPose(rest));
            world.insert(e, CurrentPose(rest));
            if let Some(mut members) = owners.remove(&source.id) {
                members.sort();
                world.insert(e, Members(members));
            }
            let mut dynamic = BTreeMap::new();
            for (name, data) in &source.components {
                let value = Value::Object(data.clone());
                let parsed = match name.as_str() {
                    "keyframe_animation" => {
                        editor_schema::parse::<KeyframeAnimation>(&value).map(|a| {
                            world.insert(e, Track::new(&a));
                            world.insert(e, a);
                        })
                    }
                    "player_start" => editor_schema::parse::<PlayerStart>(&value).map(|p| {
                        world.insert(e, p);
                    }),
                    _ => {
                        dynamic.insert(name.clone(), value);
                        Ok(())
                    }
                };
                if let Err(error) = parsed {
                    warnings.push(LoadWarning::BadComponent {
                        entity: source.id,
                        component: name.clone(),
                        error: error.to_string(),
                    });
                }
            }
            if !dynamic.is_empty() {
                world.insert(e, Dynamic(dynamic));
            }
        }

        world.insert_resource(Time::default());
        world.insert_resource(StaticGeometry {
            objects: static_objects,
        });
        let mut schedule = Schedule::new();
        schedule.add(animate);
        let mut sim = Self {
            world,
            schedule,
            by_source,
            excluded,
        };
        sim.schedule.run(&mut sim.world);
        (sim, warnings)
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    pub fn entity(&self, id: EntityId) -> Option<Entity> {
        self.by_source.get(&id).copied()
    }

    pub fn time(&self) -> f64 {
        self.world.resource::<Time>().map_or(0.0, |t| t.elapsed)
    }

    pub fn step(&mut self, dt: f64) {
        if let Some(time) = self.world.resource_mut::<Time>() {
            time.dt = dt;
            time.elapsed += dt;
        }
        self.schedule.run(&mut self.world);
    }

    pub fn set_time(&mut self, elapsed: f64) {
        if let Some(time) = self.world.resource_mut::<Time>() {
            time.dt = 0.0;
            time.elapsed = elapsed;
        }
        self.schedule.run(&mut self.world);
    }

    pub fn static_geometry(&self) -> &[ObjectId] {
        self.world
            .resource::<StaticGeometry>()
            .map_or(&[], |s| s.objects.as_slice())
    }

    /// Objects that are not part of the simulation (on layers marked omit_from_export).
    pub fn excluded(&self) -> &HashSet<ObjectId> {
        &self.excluded
    }

    /// For each entity that owns geometry, the transform from its rest pose to its current pose.
    pub fn poses(&self) -> HashMap<EntityId, DAffine3> {
        self.world
            .iter::<Members>()
            .filter_map(|(e, _)| {
                let source = self.world.get::<Source>(e)?.0;
                let rest = self.world.get::<RestPose>(e)?.0.to_affine();
                let now = self.world.get::<CurrentPose>(e)?.0.to_affine();
                Some((source, now * rest.inverse()))
            })
            .collect()
    }

    /// The pose of the lowest-id entity with `player_start`.
    pub fn spawn(&self) -> Option<Pose> {
        self.world
            .iter::<PlayerStart>()
            .filter_map(|(e, _)| {
                Some((
                    self.world.get::<Source>(e)?.0,
                    self.world.get::<RestPose>(e)?.0,
                ))
            })
            .min_by_key(|(id, _)| *id)
            .map(|(_, pose)| pose)
    }
}
