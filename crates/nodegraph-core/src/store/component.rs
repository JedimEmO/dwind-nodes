use super::entity::{EntityId, Generation};

#[derive(Clone)]
pub struct ComponentStore<T: Clone> {
    data: Vec<Option<T>>,
    generations: Vec<Generation>,
}

impl<T: Clone> ComponentStore<T> {
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            generations: Vec::new(),
        }
    }

    fn ensure_capacity(&mut self, index: usize) {
        while self.data.len() <= index {
            self.data.push(None);
            self.generations.push(Generation::default());
        }
    }

    pub fn insert(&mut self, id: EntityId, component: T) {
        let idx = id.index as usize;
        self.ensure_capacity(idx);
        self.data[idx] = Some(component);
        self.generations[idx] = id.generation;
    }

    pub fn get(&self, id: EntityId) -> Option<&T> {
        let idx = id.index as usize;
        if idx >= self.data.len() || self.generations[idx] != id.generation {
            return None;
        }
        self.data[idx].as_ref()
    }

    pub fn get_mut(&mut self, id: EntityId) -> Option<&mut T> {
        let idx = id.index as usize;
        if idx >= self.data.len() || self.generations[idx] != id.generation {
            return None;
        }
        self.data[idx].as_mut()
    }

    pub fn remove(&mut self, id: EntityId) -> Option<T> {
        let idx = id.index as usize;
        if idx >= self.data.len() || self.generations[idx] != id.generation {
            return None;
        }
        self.data[idx].take()
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Get component at raw index (for iteration). Returns None if slot is empty.
    pub fn get_by_index(&self, index: usize) -> Option<(&Generation, &T)> {
        if index >= self.data.len() {
            return None;
        }
        self.data[index].as_ref().map(|d| (&self.generations[index], d))
    }
}

impl<T: Clone> Default for ComponentStore<T> {
    fn default() -> Self {
        Self::new()
    }
}
