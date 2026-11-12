use std::any::{Any, TypeId};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Entity {
    pub index: u32,
    pub generation: u32,
}

trait AnyStorage {
    fn remove(&mut self, index: usize);
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

struct Storage<T>(Vec<Option<T>>);

impl<T: 'static> AnyStorage for Storage<T> {
    fn remove(&mut self, index: usize) {
        if let Some(slot) = self.0.get_mut(index) {
            *slot = None;
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[derive(Default)]
pub struct World {
    generations: Vec<u32>,
    alive: Vec<bool>,
    free: Vec<u32>,
    storages: HashMap<TypeId, Box<dyn AnyStorage>>,
    resources: HashMap<TypeId, Box<dyn Any>>,
}

impl World {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn spawn(&mut self) -> Entity {
        if let Some(index) = self.free.pop() {
            self.alive[index as usize] = true;
            return Entity {
                index,
                generation: self.generations[index as usize],
            };
        }
        self.generations.push(0);
        self.alive.push(true);
        Entity {
            index: (self.generations.len() - 1) as u32,
            generation: 0,
        }
    }

    pub fn is_alive(&self, e: Entity) -> bool {
        self.alive.get(e.index as usize) == Some(&true)
            && self.generations[e.index as usize] == e.generation
    }

    pub fn despawn(&mut self, e: Entity) -> bool {
        if !self.is_alive(e) {
            return false;
        }
        for storage in self.storages.values_mut() {
            storage.remove(e.index as usize);
        }
        self.alive[e.index as usize] = false;
        self.generations[e.index as usize] += 1;
        self.free.push(e.index);
        true
    }

    pub fn len(&self) -> usize {
        self.alive.iter().filter(|a| **a).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn storage<T: 'static>(&self) -> Option<&Vec<Option<T>>> {
        self.storages
            .get(&TypeId::of::<T>())
            .and_then(|s| s.as_any().downcast_ref::<Storage<T>>())
            .map(|s| &s.0)
    }

    fn storage_mut<T: 'static>(&mut self) -> &mut Vec<Option<T>> {
        &mut self
            .storages
            .entry(TypeId::of::<T>())
            .or_insert_with(|| Box::new(Storage::<T>(Vec::new())))
            .as_any_mut()
            .downcast_mut::<Storage<T>>()
            .expect("storage matches its type id")
            .0
    }

    /// Returns false, and stores nothing, if the entity is dead.
    pub fn insert<T: 'static>(&mut self, e: Entity, value: T) -> bool {
        if !self.is_alive(e) {
            return false;
        }
        let storage = self.storage_mut::<T>();
        if storage.len() <= e.index as usize {
            storage.resize_with(e.index as usize + 1, || None);
        }
        storage[e.index as usize] = Some(value);
        true
    }

    pub fn get<T: 'static>(&self, e: Entity) -> Option<&T> {
        if !self.is_alive(e) {
            return None;
        }
        self.storage::<T>()?.get(e.index as usize)?.as_ref()
    }

    pub fn get_mut<T: 'static>(&mut self, e: Entity) -> Option<&mut T> {
        if !self.is_alive(e) {
            return None;
        }
        self.storage_mut::<T>().get_mut(e.index as usize)?.as_mut()
    }

    pub fn remove<T: 'static>(&mut self, e: Entity) -> Option<T> {
        if !self.is_alive(e) {
            return None;
        }
        self.storage_mut::<T>().get_mut(e.index as usize)?.take()
    }

    /// Every live entity carrying a `T`, in index order.
    pub fn iter<T: 'static>(&self) -> impl Iterator<Item = (Entity, &T)> {
        self.storage::<T>()
            .into_iter()
            .flat_map(|s| s.iter().enumerate())
            .filter_map(|(i, slot)| {
                slot.as_ref().map(|v| {
                    (
                        Entity {
                            index: i as u32,
                            generation: self.generations[i],
                        },
                        v,
                    )
                })
            })
    }

    pub fn entities_with<T: 'static>(&self) -> Vec<Entity> {
        self.iter::<T>().map(|(e, _)| e).collect()
    }

    pub fn insert_resource<R: 'static>(&mut self, value: R) {
        self.resources.insert(TypeId::of::<R>(), Box::new(value));
    }

    pub fn resource<R: 'static>(&self) -> Option<&R> {
        self.resources
            .get(&TypeId::of::<R>())
            .and_then(|r| r.downcast_ref())
    }

    pub fn resource_mut<R: 'static>(&mut self) -> Option<&mut R> {
        self.resources
            .get_mut(&TypeId::of::<R>())
            .and_then(|r| r.downcast_mut())
    }
}

pub type System = fn(&mut World);

#[derive(Default)]
pub struct Schedule {
    systems: Vec<System>,
}

impl Schedule {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, system: System) -> &mut Self {
        self.systems.push(system);
        self
    }

    pub fn run(&self, world: &mut World) {
        for system in &self.systems {
            system(world);
        }
    }
}
