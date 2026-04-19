use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use futures_signals::map_ref;
use futures_signals::signal::SignalExt;
use futures_signals::signal_vec::SignalVecExt;

use nodegraph_core::graph::connection::ConnectionEndpoints;
use nodegraph_core::graph::port::{PortDirection, PortSocketType};
use nodegraph_core::types::socket_type::SocketType;
use nodegraph_core::EntityId;
use nodegraph_render::GraphSignals;

use crate::computation::ComputationRegistry;
use crate::conversion::ConversionRegistry;
use crate::params::ParamStore;
use crate::store::{TypedValueStore, ValueStore};
use crate::value::ParamValue;

/// Central orchestrator for node-graph runtime. Owns the per-type value
/// stores, the conversion registry, and the per-node-type computation
/// registry. Reacts to structural graph changes (nodes added/removed,
/// connections made/broken) via a reconciliation watcher that subscribes
/// to `GraphSignals::node_list` / `connection_list`.
///
/// Applications:
/// 1. Register every `ParamValue` type they use via [`Runtime::register_value_type`].
/// 2. Register conversions between primitive types via [`Runtime::conversions`].
/// 3. Register a [`NodeComputation`](crate::computation::NodeComputation) per node type.
/// 4. Call [`Runtime::initial_setup`] then [`Runtime::spawn_reconciliation_watcher`].
pub struct Runtime {
    gs: Rc<GraphSignals>,
    params: Rc<ParamStore>,

    /// Per-`SocketType` value store. Populated by `register_value_type::<T>`.
    stores: RefCell<HashMap<SocketType, Rc<dyn ValueStore>>>,

    conversions: ConversionRegistry,
    computations: ComputationRegistry,

    /// Fallback computation for nodes with a `SubgraphRoot` component
    /// (i.e. group nodes) whose `type_id` isn't directly registered.
    /// Apps that support groups install their `Group` computation here.
    group_computation: RefCell<Option<Rc<dyn crate::computation::NodeComputation>>>,

    /// Per-node alive flag. Flipping to `false` cancels the node's spawned
    /// computation task on the next signal emission.
    node_alive: RefCell<HashMap<EntityId, Rc<Cell<bool>>>>,
    /// Reverse lookup: connection id → target port. Needed because by the
    /// time we see a disconnect, the connection entity is already gone.
    conn_targets: RefCell<HashMap<EntityId, EntityId>>,
    /// Per-connection alive flag for conversion-bridge tasks.
    conn_alive: RefCell<HashMap<EntityId, Rc<Cell<bool>>>>,
}

impl Runtime {
    pub fn new(gs: Rc<GraphSignals>, params: Rc<ParamStore>) -> Rc<Self> {
        Rc::new(Self {
            gs,
            params,
            stores: RefCell::new(HashMap::new()),
            conversions: ConversionRegistry::new(),
            computations: ComputationRegistry::new(),
            group_computation: RefCell::new(None),
            node_alive: RefCell::new(HashMap::new()),
            conn_targets: RefCell::new(HashMap::new()),
            conn_alive: RefCell::new(HashMap::new()),
        })
    }

    pub fn gs(&self) -> &Rc<GraphSignals> {
        &self.gs
    }

    pub fn params(&self) -> &Rc<ParamStore> {
        &self.params
    }

    pub fn conversions(&self) -> &ConversionRegistry {
        &self.conversions
    }

    pub fn computations(&self) -> &ComputationRegistry {
        &self.computations
    }

    /// Install a fallback computation used by `setup_node` for nodes that
    /// carry a `SubgraphRoot` component (group nodes). Apps that support
    /// groups typically install their `Group` `NodeComputation` here.
    pub fn set_group_computation(&self, comp: Rc<dyn crate::computation::NodeComputation>) {
        self.group_computation.borrow_mut().replace(comp);
    }

    /// Register a value type. The runtime creates a `TypedValueStore<T>`
    /// and keys it by `T::SOCKET_TYPE`, so subsequent connection handling
    /// and `get_output::<T>` lookups find it.
    ///
    /// Calling this twice for the same `T` is a silent no-op. Calling it
    /// for a different `T` that reuses an already-bound `SOCKET_TYPE`
    /// **panics** — each `SocketType` must map to a single Rust type, or
    /// later `get_output::<T>` / `handle_connect` dispatch will silently
    /// find the wrong store. Fail loud at registration instead.
    pub fn register_value_type<T: ParamValue + Default>(&self) {
        let key = T::SOCKET_TYPE;
        let mut stores = self.stores.borrow_mut();
        if let Some(existing) = stores.get(&key) {
            if existing
                .as_any()
                .downcast_ref::<TypedValueStore<T>>()
                .is_none()
            {
                panic!(
                    "ParamValue `{}` shares SocketType::{:?} with an already-registered \
                     type; each SocketType must map to exactly one Rust type",
                    std::any::type_name::<T>(),
                    key,
                );
            }
            return;
        }
        stores.insert(
            key,
            Rc::new(TypedValueStore::<T>::new()) as Rc<dyn ValueStore>,
        );
    }

    /// Fetch an output `Mutable<T>` created during node setup. Downstream
    /// code uses this to observe a node's output reactively.
    pub fn get_output<T: ParamValue + Default>(
        &self,
        port_id: EntityId,
    ) -> Option<futures_signals::signal::Mutable<T>> {
        let store = self.stores.borrow().get(&T::SOCKET_TYPE).cloned()?;
        let typed = store.as_any().downcast_ref::<TypedValueStore<T>>()?;
        typed.get_output(port_id)
    }

    /// Fetch the source-selector `Mutable<Option<Mutable<T>>>` for this
    /// input port. NodeCtx uses this when building an input signal that
    /// switches between upstream and param fallback.
    pub fn get_source<T: ParamValue + Default>(
        &self,
        port_id: EntityId,
    ) -> Option<futures_signals::signal::Mutable<Option<futures_signals::signal::Mutable<T>>>> {
        let store = self.stores.borrow().get(&T::SOCKET_TYPE).cloned()?;
        let typed = store.as_any().downcast_ref::<TypedValueStore<T>>()?;
        typed.get_source(port_id)
    }

    /// Build a signal for an input port that switches between the upstream
    /// source (when connected) and the param `Mutable` fallback (seeded
    /// with `default` on first access). Works without a `NodeCtx`, so
    /// out-of-computation consumers like preview canvases can use it.
    pub fn input_signal_or<T: ParamValue + Default>(
        &self,
        port_id: EntityId,
        default: T,
    ) -> crate::computation::BoxSignal<T> {
        let source = match self.get_source::<T>(port_id) {
            Some(s) => s,
            None => return Box::pin(futures_signals::signal::always(default)),
        };
        let param = self.params.get::<T>(port_id, default);
        Box::pin(
            source
                .signal_cloned()
                .switch(move |opt| -> crate::computation::BoxSignal<T> {
                    match opt {
                        Some(upstream) => Box::pin(upstream.signal_cloned()),
                        None => Box::pin(param.signal_cloned()),
                    }
                }),
        )
    }

    /// Convenience `input_signal_or` using `T::default()`. The `_default`
    /// suffix is there so call sites can't easily confuse this with an
    /// explicit-default variant: for scalars like `f64` / `i64` / `bool`,
    /// `T::default()` is `0` / `0` / `false`, which is rarely what an app
    /// actually wants — prefer `input_signal_or` with an explicit value.
    /// This method is still useful for `Texture` / `[u8; 4]` where the
    /// zero-default (black texture / black color) matches "no input."
    pub fn input_signal_default<T: ParamValue + Default>(
        &self,
        port_id: EntityId,
    ) -> crate::computation::BoxSignal<T> {
        self.input_signal_or(port_id, T::default())
    }

    /// Ensure a source/output entry exists for `port_id`. Idempotent.
    fn ensure_store_entry(&self, port_id: EntityId, socket: SocketType, is_input: bool) {
        let store = self.stores.borrow().get(&socket).cloned();
        if let Some(store) = store {
            if is_input {
                store.setup_source(port_id);
            } else {
                store.setup_output(port_id);
            }
        }
    }

    /// Collect port info for `node_id` as (port_id, direction, socket, label).
    fn collect_ports(
        &self,
        node_id: EntityId,
    ) -> Vec<(EntityId, PortDirection, SocketType, String)> {
        use nodegraph_core::graph::port::PortLabel;
        self.gs.with_graph(|g| {
            g.node_ports(node_id)
                .iter()
                .map(|&pid| {
                    let dir = g
                        .world
                        .get::<PortDirection>(pid)
                        .copied()
                        .unwrap_or(PortDirection::Input);
                    let st = g
                        .world
                        .get::<PortSocketType>(pid)
                        .map(|s| s.0)
                        .unwrap_or(SocketType::Float);
                    let label = g
                        .world
                        .get::<PortLabel>(pid)
                        .map(|l| l.0.clone())
                        .unwrap_or_default();
                    (pid, dir, st, label)
                })
                .collect()
        })
    }

    fn get_type_id(&self, node_id: EntityId) -> String {
        use nodegraph_core::graph::node::NodeTypeId;
        self.gs.with_graph(|g| {
            g.world
                .get::<NodeTypeId>(node_id)
                .map(|t| t.0.clone())
                .unwrap_or_default()
        })
    }

    /// Register an alive flag + seed value stores, then dispatch to the
    /// matching `NodeComputation` if registered.
    pub fn setup_node(self: &Rc<Self>, node_id: EntityId) {
        if self.node_alive.borrow().contains_key(&node_id) {
            return;
        }

        let type_id = self.get_type_id(node_id);
        let ports = self.collect_ports(node_id);

        let alive = Rc::new(Cell::new(true));
        self.node_alive.borrow_mut().insert(node_id, alive.clone());

        // Seed source selectors for input ports and output Mutables for
        // output ports, so handle_connect / downstream consumers can look
        // them up later.
        for &(pid, dir, st, _) in &ports {
            self.ensure_store_entry(pid, st, dir == PortDirection::Input);
        }

        // Dispatch to the per-type computation spawner if one is registered;
        // otherwise fall back to the installed group computation for nodes
        // carrying a `SubgraphRoot` component. Other unknown types silently
        // fall through.
        if let Some(comp) = self.computations.get(&type_id) {
            let ctx = crate::computation::NodeCtx::new(self, node_id, &type_id, &ports);
            comp.spawn(&ctx, alive);
        } else if self.group_computation.borrow().is_some() {
            let is_group = self.gs.with_graph(|g| {
                g.world
                    .get::<nodegraph_core::graph::group::SubgraphRoot>(node_id)
                    .is_some()
            });
            if is_group {
                let comp = self.group_computation.borrow().as_ref().cloned();
                if let Some(comp) = comp {
                    let ctx = crate::computation::NodeCtx::new(self, node_id, &type_id, &ports);
                    comp.spawn(&ctx, alive);
                }
            }
        }
    }

    /// Flip the alive flag so the node's spawned task becomes inert. Does
    /// not prune output Mutables — downstream consumers may still reference
    /// them until the next `reconcile()` run rebuilds connections.
    pub fn teardown_node(&self, node_id: EntityId) {
        if let Some(alive) = self.node_alive.borrow_mut().remove(&node_id) {
            alive.set(false);
        }
    }

    pub fn handle_connect(&self, src_port: EntityId, tgt_port: EntityId, conn_id: EntityId) {
        self.conn_targets.borrow_mut().insert(conn_id, tgt_port);

        let (src_type, tgt_type) = self.gs.with_graph(|g| {
            (
                g.world.get::<PortSocketType>(src_port).map(|s| s.0),
                g.world.get::<PortSocketType>(tgt_port).map(|s| s.0),
            )
        });
        let (src_type, tgt_type) = match (src_type, tgt_type) {
            (Some(s), Some(t)) => (s, t),
            _ => return,
        };

        if src_type == tgt_type {
            if let Some(store) = self.stores.borrow().get(&src_type).cloned() {
                store.wire_same(src_port, tgt_port);
            }
            return;
        }

        // Cross-type: ask the conversion registry for a bridge spawner.
        let src_store = match self.stores.borrow().get(&src_type).cloned() {
            Some(s) => s,
            None => return,
        };
        let tgt_store = match self.stores.borrow().get(&tgt_type).cloned() {
            Some(s) => s,
            None => return,
        };
        let src_mutable_any = match src_store.get_output_any(src_port) {
            Some(b) => b,
            None => return,
        };
        if let Some((bridge, alive)) =
            self.conversions
                .spawn(src_type, tgt_type, src_mutable_any.as_ref())
        {
            self.conn_alive.borrow_mut().insert(conn_id, alive);
            tgt_store.plug_bridge(tgt_port, bridge);
        }
    }

    pub fn handle_disconnect(&self, conn_id: EntityId) {
        let tgt_port = match self.conn_targets.borrow_mut().remove(&conn_id) {
            Some(p) => p,
            None => return,
        };

        if let Some(alive) = self.conn_alive.borrow_mut().remove(&conn_id) {
            alive.set(false);
        }

        // Clear the target selector across all registered value types.
        // Only one store actually holds the selector for this port, but
        // it's cheap to ask each and cleaner than tracking per-connection
        // type info.
        let stores: Vec<Rc<dyn ValueStore>> = self.stores.borrow().values().cloned().collect();
        for store in stores {
            store.clear_source(tgt_port);
        }
    }

    /// Diff-based sync of `node_alive` / `conn_targets` against the current
    /// graph state. Handles undo, redo, delete, group/ungroup, and initial
    /// population.
    pub fn reconcile(self: &Rc<Self>) {
        let live_nodes: Vec<EntityId> = self.gs.node_list.lock_ref().iter().copied().collect();
        let live_conns: Vec<EntityId> =
            self.gs.connection_list.lock_ref().iter().copied().collect();

        let live_node_set: HashSet<EntityId> = live_nodes.iter().copied().collect();
        let live_conn_set: HashSet<EntityId> = live_conns.iter().copied().collect();

        let stale_nodes: Vec<EntityId> = self
            .node_alive
            .borrow()
            .keys()
            .filter(|id| !live_node_set.contains(id))
            .copied()
            .collect();
        for nid in stale_nodes {
            self.teardown_node(nid);
        }

        for &nid in &live_nodes {
            self.setup_node(nid);
        }

        let stale_conns: Vec<EntityId> = self
            .conn_targets
            .borrow()
            .keys()
            .filter(|id| !live_conn_set.contains(id))
            .copied()
            .collect();
        for cid in stale_conns {
            self.handle_disconnect(cid);
        }

        let known_conns: HashSet<EntityId> = self.conn_targets.borrow().keys().copied().collect();
        for &cid in &live_conns {
            if known_conns.contains(&cid) {
                continue;
            }
            let endpoints = self
                .gs
                .with_graph(|g| g.world.get::<ConnectionEndpoints>(cid).cloned());
            if let Some(ep) = endpoints {
                self.handle_connect(ep.source_port, ep.target_port, cid);
            }
        }
    }

    pub fn initial_setup(self: &Rc<Self>) {
        self.reconcile();
    }

    /// Spawn the watcher that reconciles on any `node_list`/`connection_list` change.
    pub fn spawn_reconciliation_watcher(self: &Rc<Self>) {
        let rt = self.clone();
        wasm_bindgen_futures::spawn_local(async move {
            map_ref! {
                let _nodes = rt.gs.node_list.signal_vec_cloned().to_signal_cloned(),
                let _conns = rt.gs.connection_list.signal_vec_cloned().to_signal_cloned()
                => {}
            }
            .for_each(move |_| {
                rt.reconcile();
                async {}
            })
            .await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::ParamStore;
    use nodegraph_render::GraphSignals;
    use wasm_bindgen_test::*;

    fn make_runtime() -> (Rc<GraphSignals>, Rc<ParamStore>, Rc<Runtime>) {
        let gs = GraphSignals::new();
        let params = ParamStore::new();
        let runtime = Runtime::new(gs.clone(), params.clone());
        runtime.register_value_type::<f64>();
        runtime.register_value_type::<i64>();
        runtime.register_value_type::<bool>();
        runtime.conversions().register::<i64, f64, _>(|i| i as f64);
        (gs, params, runtime)
    }

    #[wasm_bindgen_test]
    fn same_type_connection_wires_source_selector() {
        let (gs, _params, runtime) = make_runtime();

        let (_n1, p1) = gs.add_node_typed(
            "A",
            Some("producer"),
            (0.0, 0.0),
            vec![(PortDirection::Output, SocketType::Float, "Out".into())],
        );
        let (_n2, p2) = gs.add_node_typed(
            "B",
            Some("consumer"),
            (100.0, 0.0),
            vec![(PortDirection::Input, SocketType::Float, "In".into())],
        );
        let out_port = p1[0];
        let in_port = p2[0];

        runtime.initial_setup();

        let src = runtime
            .get_output::<f64>(out_port)
            .expect("output Mutable must be created during setup_node");
        src.set(7.25);
        let sel = runtime
            .get_source::<f64>(in_port)
            .expect("input source selector must exist");
        assert!(
            sel.get_cloned().is_none(),
            "selector is None before connection"
        );

        let _conn_id = gs.connect_ports(out_port, in_port).expect("connect");
        runtime.reconcile();

        let inside = sel
            .get_cloned()
            .expect("after reconcile the selector must hold the upstream Mutable");
        assert_eq!(inside.get(), 7.25);
        src.set(9.0);
        assert_eq!(inside.get(), 9.0, "selector tracks upstream changes");
    }

    #[wasm_bindgen_test]
    fn cross_type_connection_installs_bridge() {
        let (gs, _params, runtime) = make_runtime();

        let (_n1, p1) = gs.add_node_typed(
            "IntSrc",
            Some("producer"),
            (0.0, 0.0),
            vec![(PortDirection::Output, SocketType::Int, "Out".into())],
        );
        let (_n2, p2) = gs.add_node_typed(
            "FloatSink",
            Some("consumer"),
            (100.0, 0.0),
            vec![(PortDirection::Input, SocketType::Float, "In".into())],
        );
        let out_port = p1[0];
        let in_port = p2[0];

        runtime.initial_setup();
        runtime
            .get_output::<i64>(out_port)
            .expect("int output Mutable")
            .set(42);

        let _conn_id = gs.connect_ports(out_port, in_port).expect("connect");
        runtime.reconcile();

        let sel = runtime
            .get_source::<f64>(in_port)
            .expect("float source selector must exist");
        let bridge = sel
            .get_cloned()
            .expect("cross-type connection must install a bridge Mutable");
        assert_eq!(
            bridge.get(),
            42.0,
            "bridge must carry the converted source value"
        );
    }

    #[wasm_bindgen_test]
    fn disconnect_clears_source_selector() {
        let (gs, _params, runtime) = make_runtime();

        let (_n1, p1) = gs.add_node_typed(
            "A",
            Some("producer"),
            (0.0, 0.0),
            vec![(PortDirection::Output, SocketType::Float, "Out".into())],
        );
        let (_n2, p2) = gs.add_node_typed(
            "B",
            Some("consumer"),
            (100.0, 0.0),
            vec![(PortDirection::Input, SocketType::Float, "In".into())],
        );
        let out_port = p1[0];
        let in_port = p2[0];

        runtime.initial_setup();
        let conn_id = gs.connect_ports(out_port, in_port).expect("connect");
        runtime.reconcile();

        let sel = runtime.get_source::<f64>(in_port).unwrap();
        assert!(sel.get_cloned().is_some(), "wired before disconnect");

        // Disconnect by calling handle_disconnect directly — reconcile()
        // wouldn't see the removal without mutating connection_list first.
        runtime.handle_disconnect(conn_id);
        assert!(
            sel.get_cloned().is_none(),
            "selector cleared after disconnect"
        );
    }

    #[wasm_bindgen_test]
    #[should_panic(expected = "shares SocketType")]
    fn register_value_type_panics_on_collision() {
        // Define a second f64-mapped ParamValue locally so registration
        // collides with the builtin f64 impl.
        #[derive(Clone, Default)]
        struct AltFloat(f64);
        impl crate::value::ParamValue for AltFloat {
            const SOCKET_TYPE: SocketType = SocketType::Float;
        }

        let (_gs, _params, runtime) = make_runtime();
        runtime.register_value_type::<AltFloat>();
    }

    #[wasm_bindgen_test]
    fn register_value_type_same_type_is_noop() {
        let (_gs, _params, runtime) = make_runtime();
        // Calling again with the same T must NOT panic.
        runtime.register_value_type::<f64>();
        runtime.register_value_type::<f64>();
    }
}
