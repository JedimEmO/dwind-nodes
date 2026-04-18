use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;

use futures_signals::signal::Mutable;

use nodegraph_core::EntityId;

use crate::value::ParamValue;

/// Object-safe trait that hides a `TypedValueStore<T>` behind a common
/// interface. The `Runtime` uses this so connection handling and lifecycle
/// don't need to know the concrete value type — they just look up the
/// right `dyn ValueStore` by `SocketType`.
pub trait ValueStore: 'static {
    /// Ensure a source selector exists for this input port.
    fn setup_source(&self, port_id: EntityId);
    /// Ensure an output `Mutable` exists for this output port.
    fn setup_output(&self, port_id: EntityId);
    /// Direct same-type wiring: plug the source output Mutable into the
    /// target's source selector. Does nothing if either side is missing.
    fn wire_same(&self, src_port: EntityId, tgt_port: EntityId);
    /// Set the target source selector back to `None` (disconnected).
    fn clear_source(&self, tgt_port: EntityId);
    /// Plug a type-erased bridge `Mutable<T>` (expected to match this
    /// store's `T`) into the target source selector.
    fn plug_bridge(&self, tgt_port: EntityId, bridge: Box<dyn Any>);
    /// Return a type-erased clone of the output `Mutable<T>` for
    /// conversion-bridge construction. `None` if the port has no entry.
    fn get_output_any(&self, port_id: EntityId) -> Option<Box<dyn Any>>;
    /// Access the concrete store for typed downcast (`Runtime::get_output::<T>`).
    fn as_any(&self) -> &dyn Any;
}

/// Concrete per-type store. One of these lives behind each `dyn ValueStore`
/// the runtime holds.
pub struct TypedValueStore<T: ParamValue + Default> {
    outputs: RefCell<HashMap<EntityId, Mutable<T>>>,
    sources: RefCell<HashMap<EntityId, Mutable<Option<Mutable<T>>>>>,
}

impl<T: ParamValue + Default> Default for TypedValueStore<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: ParamValue + Default> TypedValueStore<T> {
    pub fn new() -> Self {
        Self {
            outputs: RefCell::new(HashMap::new()),
            sources: RefCell::new(HashMap::new()),
        }
    }

    pub fn get_or_create_output(&self, port_id: EntityId) -> Mutable<T> {
        self.outputs
            .borrow_mut()
            .entry(port_id)
            .or_insert_with(|| Mutable::new(T::default()))
            .clone()
    }

    pub fn get_or_create_source(&self, port_id: EntityId) -> Mutable<Option<Mutable<T>>> {
        self.sources
            .borrow_mut()
            .entry(port_id)
            .or_insert_with(|| Mutable::new(None))
            .clone()
    }

    pub fn get_output(&self, port_id: EntityId) -> Option<Mutable<T>> {
        self.outputs.borrow().get(&port_id).cloned()
    }

    pub fn get_source(&self, port_id: EntityId) -> Option<Mutable<Option<Mutable<T>>>> {
        self.sources.borrow().get(&port_id).cloned()
    }
}

impl<T: ParamValue + Default> ValueStore for TypedValueStore<T> {
    fn setup_source(&self, port_id: EntityId) {
        self.get_or_create_source(port_id);
    }

    fn setup_output(&self, port_id: EntityId) {
        self.get_or_create_output(port_id);
    }

    fn wire_same(&self, src_port: EntityId, tgt_port: EntityId) {
        let src = match self.get_output(src_port) {
            Some(m) => m,
            None => return,
        };
        if let Some(selector) = self.get_source(tgt_port) {
            selector.set(Some(src));
        }
    }

    fn clear_source(&self, tgt_port: EntityId) {
        if let Some(selector) = self.get_source(tgt_port) {
            selector.set(None);
        }
    }

    fn plug_bridge(&self, tgt_port: EntityId, bridge: Box<dyn Any>) {
        let bridge: Box<Mutable<T>> = match bridge.downcast() {
            Ok(b) => b,
            Err(_) => return,
        };
        if let Some(selector) = self.get_source(tgt_port) {
            selector.set(Some(*bridge));
        }
    }

    fn get_output_any(&self, port_id: EntityId) -> Option<Box<dyn Any>> {
        self.get_output(port_id)
            .map(|m| Box::new(m) as Box<dyn Any>)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodegraph_core::store::World;
    use wasm_bindgen_test::*;

    #[wasm_bindgen_test]
    fn output_get_or_create_is_idempotent() {
        let mut world = World::new();
        let pid = world.spawn();

        let store = TypedValueStore::<f64>::new();
        let a = store.get_or_create_output(pid);
        a.set(1.5);
        let b = store.get_or_create_output(pid);
        assert_eq!(b.get(), 1.5, "second call must return the same Mutable");
    }

    #[wasm_bindgen_test]
    fn wire_same_plugs_source_into_target() {
        let mut world = World::new();
        let src_port = world.spawn();
        let tgt_port = world.spawn();

        let store = TypedValueStore::<i64>::new();
        let src = store.get_or_create_output(src_port);
        let tgt_sel = store.get_or_create_source(tgt_port);
        src.set(7);

        store.wire_same(src_port, tgt_port);
        let inside = tgt_sel.get_cloned().expect("selector must be Some");
        assert_eq!(inside.get(), 7);
    }

    #[wasm_bindgen_test]
    fn clear_source_resets_to_none() {
        let mut world = World::new();
        let src_port = world.spawn();
        let tgt_port = world.spawn();

        let store = TypedValueStore::<bool>::new();
        let _src = store.get_or_create_output(src_port);
        let tgt_sel = store.get_or_create_source(tgt_port);
        store.wire_same(src_port, tgt_port);
        assert!(tgt_sel.get_cloned().is_some());

        store.clear_source(tgt_port);
        assert!(tgt_sel.get_cloned().is_none());
    }

    #[wasm_bindgen_test]
    fn plug_bridge_accepts_matching_type() {
        let mut world = World::new();
        let tgt = world.spawn();

        let store = TypedValueStore::<f64>::new();
        let sel = store.get_or_create_source(tgt);

        let bridge = Mutable::new(4.2_f64);
        store.plug_bridge(tgt, Box::new(bridge.clone()) as Box<dyn std::any::Any>);
        let inside = sel.get_cloned().expect("plugged");
        assert_eq!(inside.get(), 4.2);
        bridge.set(5.0);
        assert_eq!(inside.get(), 5.0, "bridge clone must share the value");
    }

    #[wasm_bindgen_test]
    fn plug_bridge_ignores_wrong_type() {
        let mut world = World::new();
        let tgt = world.spawn();

        let store = TypedValueStore::<f64>::new();
        let sel = store.get_or_create_source(tgt);
        assert!(sel.get_cloned().is_none());

        let wrong_bridge = Mutable::new(1_i64);
        store.plug_bridge(tgt, Box::new(wrong_bridge) as Box<dyn std::any::Any>);
        assert!(
            sel.get_cloned().is_none(),
            "wrong-type bridge must be dropped, selector unchanged"
        );
    }
}
