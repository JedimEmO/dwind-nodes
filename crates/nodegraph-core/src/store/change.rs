use super::entity::EntityId;
use std::any::TypeId;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ChangeRecord {
    pub entity: EntityId,
    pub component_type: TypeId,
}

pub struct ChangeTracker {
    changes: HashSet<ChangeRecord>,
}

impl ChangeTracker {
    pub fn new() -> Self {
        Self {
            changes: HashSet::new(),
        }
    }

    pub fn record<T: 'static>(&mut self, entity: EntityId) {
        self.changes.insert(ChangeRecord {
            entity,
            component_type: TypeId::of::<T>(),
        });
    }

    pub fn changes(&self) -> &HashSet<ChangeRecord> {
        &self.changes
    }

    pub fn changed_entities<T: 'static>(&self) -> impl Iterator<Item = EntityId> + '_ {
        let type_id = TypeId::of::<T>();
        self.changes
            .iter()
            .filter(move |r| r.component_type == type_id)
            .map(|r| r.entity)
    }

    pub fn has_changes(&self) -> bool {
        !self.changes.is_empty()
    }

    pub fn clear(&mut self) {
        self.changes.clear();
    }
}

impl Default for ChangeTracker {
    fn default() -> Self {
        Self::new()
    }
}
