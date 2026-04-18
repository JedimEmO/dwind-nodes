use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use futures_signals::signal::Mutable;

use nodegraph_core::types::socket_type::SocketType;
use nodegraph_core::EntityId;

use crate::value::ParamValue;

/// Per-`ParamValue`-type bucket: `RefCell<HashMap<EntityId, Mutable<T>>>`
/// dressed up so `ParamStore` can store all its buckets in one
/// `HashMap<SocketType, Rc<dyn ParamBucket>>` and still do type-agnostic
/// operations (like port-id migration).
trait ParamBucket {
    fn migrate(&self, old_to_new: &HashMap<EntityId, EntityId>);
    fn as_any(&self) -> &dyn Any;
}

struct TypedBucket<T: ParamValue> {
    values: RefCell<HashMap<EntityId, Mutable<T>>>,
}

impl<T: ParamValue> TypedBucket<T> {
    fn new() -> Self {
        Self {
            values: RefCell::new(HashMap::new()),
        }
    }
}

impl<T: ParamValue> ParamBucket for TypedBucket<T> {
    fn migrate(&self, old_to_new: &HashMap<EntityId, EntityId>) {
        let mut m = self.values.borrow_mut();
        for (old, new) in old_to_new {
            if let Some(v) = m.get(old).cloned() {
                m.insert(*new, v);
            }
        }
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Storage for editable per-port values. The widget layer writes into these
/// `Mutable<T>` instances; the `Runtime` reads them when building input
/// signal chains and when spawning const-style nodes.
///
/// Storage is type-erased by `SocketType`, so a single `ParamStore` handles
/// every value type the application registers without per-type boilerplate.
pub struct ParamStore {
    buckets: RefCell<HashMap<SocketType, Rc<dyn ParamBucket>>>,
}

impl ParamStore {
    pub fn new() -> Rc<Self> {
        Rc::new(Self {
            buckets: RefCell::new(HashMap::new()),
        })
    }

    fn with_bucket<T, R>(&self, f: impl FnOnce(&TypedBucket<T>) -> R) -> R
    where
        T: ParamValue,
    {
        let mut buckets = self.buckets.borrow_mut();
        let bucket = buckets
            .entry(T::SOCKET_TYPE)
            .or_insert_with(|| Rc::new(TypedBucket::<T>::new()) as Rc<dyn ParamBucket>)
            .clone();
        drop(buckets);
        let typed = bucket.as_any().downcast_ref::<TypedBucket<T>>().expect(
            "bucket type mismatch for SocketType — two ParamValue types share a SocketType",
        );
        f(typed)
    }

    /// Return the editable `Mutable<T>` for this port, creating it with
    /// `default` if this is the first access. Subsequent calls ignore the
    /// `default` and return the existing `Mutable`.
    pub fn get<T: ParamValue>(&self, port_id: EntityId, default: T) -> Mutable<T> {
        self.with_bucket::<T, _>(|b| {
            b.values
                .borrow_mut()
                .entry(port_id)
                .or_insert_with(|| Mutable::new(default))
                .clone()
        })
    }

    /// Like `get` but does not create a `Mutable` if none exists.
    pub fn get_existing<T: ParamValue>(&self, port_id: EntityId) -> Option<Mutable<T>> {
        let bucket = self.buckets.borrow().get(&T::SOCKET_TYPE).cloned()?;
        let typed = bucket.as_any().downcast_ref::<TypedBucket<T>>()?;
        let out = typed.values.borrow().get(&port_id).cloned();
        out
    }

    /// Snapshot the current value of every port that stores a `T`. Plain
    /// data (no `Mutable`/`Rc`), safe to pass across eval boundaries.
    pub fn snapshot_type<T: ParamValue>(&self) -> HashMap<EntityId, T> {
        let bucket = match self.buckets.borrow().get(&T::SOCKET_TYPE).cloned() {
            Some(b) => b,
            None => return HashMap::new(),
        };
        let typed = match bucket.as_any().downcast_ref::<TypedBucket<T>>() {
            Some(t) => t,
            None => return HashMap::new(),
        };
        let out: HashMap<EntityId, T> = typed
            .values
            .borrow()
            .iter()
            .map(|(&k, v)| (k, v.get_cloned()))
            .collect();
        out
    }

    /// Remap every stored `Mutable<T>` from its old `EntityId` to the new
    /// one. Used after group / ungroup, which recreates ports with fresh ids.
    pub fn migrate_ports(&self, old_to_new: &HashMap<EntityId, EntityId>) {
        let buckets: Vec<Rc<dyn ParamBucket>> = self.buckets.borrow().values().cloned().collect();
        for bucket in buckets {
            bucket.migrate(old_to_new);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodegraph_core::store::World;
    use wasm_bindgen_test::*;

    #[wasm_bindgen_test]
    fn get_returns_same_mutable() {
        let mut world = World::new();
        let id = world.spawn();

        let store = ParamStore::new();
        let a = store.get::<f64>(id, 1.5);
        let b = store.get::<f64>(id, 99.0); // default ignored
        a.set(3.5);
        assert_eq!(b.get(), 3.5);
    }

    #[wasm_bindgen_test]
    fn get_separates_types_per_port() {
        let mut world = World::new();
        let id = world.spawn();

        let store = ParamStore::new();
        let f = store.get::<f64>(id, 1.0);
        let i = store.get::<i64>(id, 7);
        f.set(2.0);
        i.set(42);
        assert_eq!(store.get::<f64>(id, 0.0).get(), 2.0);
        assert_eq!(store.get::<i64>(id, 0).get(), 42);
    }

    #[wasm_bindgen_test]
    fn get_existing_returns_none_before_first_write() {
        let mut world = World::new();
        let id = world.spawn();
        let store = ParamStore::new();
        assert!(store.get_existing::<f64>(id).is_none());
        let _ = store.get::<f64>(id, 1.0);
        assert!(store.get_existing::<f64>(id).is_some());
    }

    #[wasm_bindgen_test]
    fn migrate_ports_copies_across_types() {
        let mut world = World::new();
        let old_a = world.spawn();
        let new_a = world.spawn();
        let old_b = world.spawn();
        let new_b = world.spawn();

        let store = ParamStore::new();
        store.get::<f64>(old_a, 0.0).set(2.5);
        store.get::<bool>(old_b, false).set(true);

        let mut map = HashMap::new();
        map.insert(old_a, new_a);
        map.insert(old_b, new_b);
        store.migrate_ports(&map);

        assert_eq!(store.get::<f64>(new_a, 0.0).get(), 2.5);
        assert!(store.get::<bool>(new_b, false).get());
    }

    #[wasm_bindgen_test]
    fn snapshot_type_copies_values() {
        let mut world = World::new();
        let id1 = world.spawn();
        let id2 = world.spawn();

        let store = ParamStore::new();
        store.get::<i64>(id1, 0).set(10);
        store.get::<i64>(id2, 0).set(20);

        let snap = store.snapshot_type::<i64>();
        assert_eq!(snap.get(&id1), Some(&10));
        assert_eq!(snap.get(&id2), Some(&20));
        assert_eq!(snap.len(), 2);
    }

    #[wasm_bindgen_test]
    fn snapshot_type_empty_when_unused() {
        let store = ParamStore::new();
        assert!(store.snapshot_type::<f64>().is_empty());
    }
}
