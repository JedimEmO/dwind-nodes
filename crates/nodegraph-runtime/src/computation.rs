use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::pin::Pin;
use std::rc::Rc;

use futures_signals::signal::{Mutable, Signal};

use nodegraph_core::graph::port::PortDirection;
use nodegraph_core::types::socket_type::SocketType;
use nodegraph_core::EntityId;

use crate::runtime::Runtime;
use crate::value::ParamValue;

/// Convenience alias for a boxed reactive signal of `T`.
pub type BoxSignal<T> = Pin<Box<dyn Signal<Item = T> + Unpin>>;

/// How a single node type computes its outputs. Implementors look up their
/// inputs via [`NodeCtx::input_signal`], combine them with `map_ref!`, and
/// drive their output `Mutable`s inside `wasm_bindgen_futures::spawn_local`.
///
/// The `alive` flag lets the runtime cancel the task when the node is
/// torn down; check `alive.get()` before every write.
pub trait NodeComputation: 'static {
    fn spawn(&self, ctx: &NodeCtx<'_>, alive: Rc<Cell<bool>>);
}

/// Per-node context passed to [`NodeComputation::spawn`]. Provides
/// input-signal and output-`Mutable` lookup by port label, plus access to
/// the underlying `Runtime` for advanced cases (group-node triggers,
/// custom signal compositions).
pub struct NodeCtx<'a> {
    runtime: &'a Rc<Runtime>,
    node_id: EntityId,
    type_id: &'a str,
    ports: &'a [(EntityId, PortDirection, SocketType, String)],
}

impl<'a> NodeCtx<'a> {
    pub fn new(
        runtime: &'a Rc<Runtime>,
        node_id: EntityId,
        type_id: &'a str,
        ports: &'a [(EntityId, PortDirection, SocketType, String)],
    ) -> Self {
        Self {
            runtime,
            node_id,
            type_id,
            ports,
        }
    }

    pub fn runtime(&self) -> &Rc<Runtime> {
        self.runtime
    }

    pub fn node_id(&self) -> EntityId {
        self.node_id
    }

    pub fn type_id(&self) -> &str {
        self.type_id
    }

    pub fn ports(&self) -> &[(EntityId, PortDirection, SocketType, String)] {
        self.ports
    }

    /// Find the port id for the given direction + label on this node.
    pub fn find_port(&self, dir: PortDirection, label: &str) -> Option<EntityId> {
        self.ports
            .iter()
            .find(|(_, d, _, l)| *d == dir && l == label)
            .map(|(id, _, _, _)| *id)
    }

    /// Build an input signal for the port with the given label. Uses the
    /// source selector if an upstream connection is present, falls back to
    /// the param `Mutable` (seeded with `default` on first access).
    pub fn input_signal_or<T: ParamValue + Default>(
        &self,
        label: &str,
        default: T,
    ) -> BoxSignal<T> {
        let pid = match self.find_port(PortDirection::Input, label) {
            Some(p) => p,
            None => return Box::pin(futures_signals::signal::always(default)),
        };
        self.runtime.input_signal_or::<T>(pid, default)
    }

    /// Same as [`input_signal_or`] but uses `T::default()` as the fallback.
    /// Prefer [`input_signal_or`] for scalar types — `T::default()` is `0` /
    /// `false` for primitives, which is rarely the intended "disconnected"
    /// value. Useful for `Texture` / `[u8; 4]`, where `T::default()` is a
    /// meaningful "empty" value.
    pub fn input_signal_default<T: ParamValue + Default>(&self, label: &str) -> BoxSignal<T> {
        self.input_signal_or(label, T::default())
    }

    /// The output `Mutable<T>` for the named output port. Writable by the
    /// computation to drive downstream signals.
    pub fn output_mutable<T: ParamValue + Default>(&self, label: &str) -> Option<Mutable<T>> {
        let pid = self.find_port(PortDirection::Output, label)?;
        self.runtime.get_output::<T>(pid)
    }

    /// The first output port of the given socket type, if any. Useful for
    /// single-output node types (e.g. generators).
    pub fn first_output_of(&self, socket: SocketType) -> Option<EntityId> {
        self.ports
            .iter()
            .find(|(_, d, s, _)| *d == PortDirection::Output && *s == socket)
            .map(|(id, _, _, _)| *id)
    }
}

/// Registry of `NodeComputation` implementors keyed by node `type_id`.
pub struct ComputationRegistry {
    map: RefCell<HashMap<String, Rc<dyn NodeComputation>>>,
}

impl Default for ComputationRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ComputationRegistry {
    pub fn new() -> Self {
        Self {
            map: RefCell::new(HashMap::new()),
        }
    }

    pub fn register(&self, type_id: impl Into<String>, comp: Rc<dyn NodeComputation>) {
        self.map.borrow_mut().insert(type_id.into(), comp);
    }

    pub fn get(&self, type_id: &str) -> Option<Rc<dyn NodeComputation>> {
        self.map.borrow().get(type_id).cloned()
    }
}
