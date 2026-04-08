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

/// Entity allocator that produces globally-unique EntityIds.
///
/// Internally uses 0-based local indices. A `start_offset` is added to produce
/// the global `EntityId.index`, ensuring disjoint ID ranges across multiple Worlds.
/// An optional `max_local` enforces block size limits without padding vectors.
#[derive(Clone)]
pub struct EntityAllocator {
    generations: Vec<Generation>,
    free_list: Vec<u32>,
    alive: Vec<bool>,
    /// Added to local index to produce the global EntityId.index.
    start_offset: u32,
    /// Maximum number of entities this allocator can hold (None = unbounded).
    max_local: Option<u32>,
}

impl EntityAllocator {
    pub fn new() -> Self {
        Self {
            generations: Vec::new(),
            free_list: Vec::new(),
            alive: Vec::new(),
            start_offset: 0,
            max_local: None,
        }
    }

    /// Create an allocator whose EntityIds start from `start` with at most
    /// `block_size` entities. No vectors are padded — only `start_offset` is stored.
    pub fn new_with_start(start: u32, block_size: u32) -> Self {
        Self {
            generations: Vec::new(),
            free_list: Vec::new(),
            alive: Vec::new(),
            start_offset: start,
            max_local: Some(block_size),
        }
    }

    pub fn start_offset(&self) -> u32 {
        self.start_offset
    }

    pub fn allocate(&mut self) -> EntityId {
        if let Some(local_index) = self.free_list.pop() {
            self.alive[local_index as usize] = true;
            EntityId {
                index: local_index + self.start_offset,
                generation: self.generations[local_index as usize],
            }
        } else {
            let local_index = self.generations.len() as u32;
            // Hard limit: prevent ID spill into another graph's reserved range.
            // The preflight in group_nodes catches most overflows early, but
            // this assert is the backstop for any path that creates entities.
            if let Some(max) = self.max_local {
                assert!(
                    local_index < max,
                    "entity allocator exceeded block range: local {local_index} >= max {max} (offset {})",
                    self.start_offset
                );
            }
            self.generations.push(Generation::new());
            self.alive.push(true);
            EntityId {
                index: local_index + self.start_offset,
                generation: Generation::new(),
            }
        }
    }

    pub fn deallocate(&mut self, id: EntityId) -> bool {
        if !self.is_alive(id) {
            return false;
        }
        let local_idx = (id.index - self.start_offset) as usize;
        self.alive[local_idx] = false;
        self.generations[local_idx].increment();
        self.free_list.push(id.index - self.start_offset);
        true
    }

    pub fn is_alive(&self, id: EntityId) -> bool {
        if id.index < self.start_offset {
            return false;
        }
        let local_idx = (id.index - self.start_offset) as usize;
        local_idx < self.generations.len()
            && self.generations[local_idx] == id.generation
            && self.alive[local_idx]
    }

    pub fn alive_count(&self) -> usize {
        self.alive.iter().filter(|&&a| a).count()
    }
}
