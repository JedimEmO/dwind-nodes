mod entity;
mod component;
mod query;
mod change;

pub use entity::{EntityId, Generation};
pub use component::ComponentStore;
pub use change::{ChangeTracker, ChangeRecord};

use std::any::{Any, TypeId};
use std::collections::HashMap;

trait CloneableStore: Any {
    fn clone_store(&self) -> Box<dyn CloneableStore>;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<T: Clone + 'static> CloneableStore for ComponentStore<T> {
    fn clone_store(&self) -> Box<dyn CloneableStore> {
        Box::new(self.clone())
    }
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

pub struct World {
    allocator: entity::EntityAllocator,
    components: HashMap<TypeId, Box<dyn CloneableStore>>,
    pub change_tracker: ChangeTracker,
}

impl World {
    pub fn new() -> Self {
        Self {
            allocator: entity::EntityAllocator::new(),
            components: HashMap::new(),
            change_tracker: ChangeTracker::new(),
        }
    }

    /// Create a World whose entity IDs start from `start_index`.
    /// Prevents ID collisions when multiple Worlds coexist.
    pub fn new_with_start(start_index: u32) -> Self {
        Self {
            allocator: entity::EntityAllocator::new_with_start(start_index),
            components: HashMap::new(),
            change_tracker: ChangeTracker::new(),
        }
    }

    pub fn spawn(&mut self) -> EntityId {
        self.allocator.allocate()
    }

    pub fn despawn(&mut self, id: EntityId) -> bool {
        if !self.allocator.is_alive(id) {
            return false;
        }
        // Component data remains in storage but becomes inaccessible:
        // generation bump makes all ComponentStore::get(old_id) return None.
        // When the slot is reused, insert() overwrites the old data.
        self.allocator.deallocate(id)
    }

    pub fn is_alive(&self, id: EntityId) -> bool {
        self.allocator.is_alive(id)
    }

    pub fn entity_count(&self) -> usize {
        self.allocator.alive_count()
    }

    fn get_store<T: Clone + 'static>(&self) -> Option<&ComponentStore<T>> {
        self.components
            .get(&TypeId::of::<T>())
            .and_then(|b| b.as_any().downcast_ref::<ComponentStore<T>>())
    }

    fn get_store_mut<T: Clone + 'static>(&mut self) -> &mut ComponentStore<T> {
        self.components
            .entry(TypeId::of::<T>())
            .or_insert_with(|| Box::new(ComponentStore::<T>::new()))
            .as_any_mut()
            .downcast_mut::<ComponentStore<T>>()
            .expect("type mismatch in component store")
    }

    pub fn insert<T: Clone + 'static>(&mut self, id: EntityId, component: T) {
        if !self.allocator.is_alive(id) {
            return;
        }
        self.change_tracker.record::<T>(id);
        self.get_store_mut::<T>().insert(id, component);
    }

    pub fn get<T: Clone + 'static>(&self, id: EntityId) -> Option<&T> {
        if !self.allocator.is_alive(id) {
            return None;
        }
        self.get_store::<T>()?.get(id)
    }

    pub fn get_mut<T: Clone + 'static>(&mut self, id: EntityId) -> Option<&mut T> {
        if !self.allocator.is_alive(id) {
            return None;
        }
        self.change_tracker.record::<T>(id);
        self.components
            .get_mut(&TypeId::of::<T>())
            .and_then(|b| b.as_any_mut().downcast_mut::<ComponentStore<T>>())
            .and_then(|store| store.get_mut(id))
    }

    pub fn remove<T: Clone + 'static>(&mut self, id: EntityId) -> Option<T> {
        if !self.allocator.is_alive(id) {
            return None;
        }
        self.change_tracker.record::<T>(id);
        self.components
            .get_mut(&TypeId::of::<T>())
            .and_then(|b| b.as_any_mut().downcast_mut::<ComponentStore<T>>())
            .and_then(|store| store.remove(id))
    }

    pub fn has<T: Clone + 'static>(&self, id: EntityId) -> bool {
        if !self.allocator.is_alive(id) {
            return false;
        }
        self.get_store::<T>()
            .map(|store| store.get(id).is_some())
            .unwrap_or(false)
    }

    pub fn query<T: Clone + 'static>(&self) -> impl Iterator<Item = (EntityId, &T)> {
        query::Query1::new(self)
    }

    pub fn query2<A: Clone + 'static, B: Clone + 'static>(&self) -> impl Iterator<Item = (EntityId, &A, &B)> {
        query::Query2::new(self)
    }
}

impl Clone for World {
    fn clone(&self) -> Self {
        Self {
            allocator: self.allocator.clone(),
            components: self.components.iter()
                .map(|(&k, v)| (k, v.clone_store()))
                .collect(),
            change_tracker: self.change_tracker.clone(),
        }
    }
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
