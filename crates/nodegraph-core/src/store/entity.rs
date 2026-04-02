use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Generation(u32);

impl Generation {
    pub fn new() -> Self {
        Self(0)
    }

    pub fn increment(&mut self) {
        self.0 += 1;
    }
}

impl Default for Generation {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntityId {
    pub index: u32,
    pub generation: Generation,
}

pub struct EntityAllocator {
    generations: Vec<Generation>,
    free_list: Vec<u32>,
    alive: Vec<bool>,
}

impl EntityAllocator {
    pub fn new() -> Self {
        Self {
            generations: Vec::new(),
            free_list: Vec::new(),
            alive: Vec::new(),
        }
    }

    pub fn allocate(&mut self) -> EntityId {
        if let Some(index) = self.free_list.pop() {
            self.alive[index as usize] = true;
            EntityId {
                index,
                generation: self.generations[index as usize],
            }
        } else {
            let index = self.generations.len() as u32;
            self.generations.push(Generation::new());
            self.alive.push(true);
            EntityId {
                index,
                generation: Generation::new(),
            }
        }
    }

    pub fn deallocate(&mut self, id: EntityId) -> bool {
        if !self.is_alive(id) {
            return false;
        }
        let idx = id.index as usize;
        self.alive[idx] = false;
        self.generations[idx].increment();
        self.free_list.push(id.index);
        true
    }

    pub fn is_alive(&self, id: EntityId) -> bool {
        let idx = id.index as usize;
        idx < self.generations.len()
            && self.generations[idx] == id.generation
            && self.alive[idx]
    }

    pub fn alive_count(&self) -> usize {
        self.alive.iter().filter(|&&a| a).count()
    }
}
