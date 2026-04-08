use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::pin::Pin;
use std::rc::Rc;

use futures_signals::map_ref;
use futures_signals::signal::{always, Mutable, Signal, SignalExt};
use futures_signals::signal_vec::SignalVecExt;

use nodegraph_core::graph::connection::ConnectionEndpoints;
use nodegraph_core::graph::group::SubgraphRoot;
use nodegraph_core::graph::node::NodeTypeId;
use nodegraph_core::graph::port::{PortDirection, PortLabel, PortSocketType};
use nodegraph_core::types::socket_type::SocketType;
use nodegraph_core::EntityId;
use nodegraph_render::GraphSignals;

use crate::eval;
use crate::params::{self, ParamStore};
use crate::texture::TextureBuffer;

type BoxSignal<T> = Pin<Box<dyn Signal<Item = T> + Unpin>>;
type TexSourceMap = RefCell<HashMap<EntityId, Mutable<Option<Mutable<Rc<TextureBuffer>>>>>>;
type ColorSourceMap = RefCell<HashMap<EntityId, Mutable<Option<Mutable<[u8; 4]>>>>>;

pub struct ReactiveEval {
    gs: Rc<GraphSignals>,
    params: Rc<ParamStore>,

    /// Output Mutables for Image-producing output ports.
    pub tex_outputs: RefCell<HashMap<EntityId, Mutable<Rc<TextureBuffer>>>>,

    /// Output Mutables for Color-producing output ports.
    pub color_outputs: RefCell<HashMap<EntityId, Mutable<[u8; 4]>>>,

    /// Per input port: dynamic source selector. None = disconnected.
    tex_sources: TexSourceMap,
    color_sources: ColorSourceMap,

    /// Per node: alive flag for cancellation.
    node_alive: RefCell<HashMap<EntityId, Rc<Cell<bool>>>>,

    /// Reverse lookup: connection_id -> target_port (needed for on_disconnect).
    conn_targets: RefCell<HashMap<EntityId, EntityId>>,
}

impl ReactiveEval {
    pub fn new(gs: Rc<GraphSignals>, params: Rc<ParamStore>) -> Rc<Self> {
        Rc::new(Self {
            gs,
            params,
            tex_outputs: RefCell::new(HashMap::new()),
            color_outputs: RefCell::new(HashMap::new()),
            tex_sources: RefCell::new(HashMap::new()),
            color_sources: RefCell::new(HashMap::new()),
            node_alive: RefCell::new(HashMap::new()),
            conn_targets: RefCell::new(HashMap::new()),
        })
    }

    // ----------------------------------------------------------------
    // Output Mutable access
    // ----------------------------------------------------------------

    fn get_or_create_tex_output(&self, port_id: EntityId) -> Mutable<Rc<TextureBuffer>> {
        self.tex_outputs
            .borrow_mut()
            .entry(port_id)
            .or_insert_with(|| Mutable::new(Rc::new(TextureBuffer::new())))
            .clone()
    }

    fn get_or_create_color_output(&self, port_id: EntityId) -> Mutable<[u8; 4]> {
        self.color_outputs
            .borrow_mut()
            .entry(port_id)
            .or_insert_with(|| Mutable::new([0, 0, 0, 255]))
            .clone()
    }

    // ----------------------------------------------------------------
    // Source selector access
    // ----------------------------------------------------------------

    fn get_or_create_tex_source(
        &self,
        port_id: EntityId,
    ) -> Mutable<Option<Mutable<Rc<TextureBuffer>>>> {
        self.tex_sources
            .borrow_mut()
            .entry(port_id)
            .or_insert_with(|| Mutable::new(None))
            .clone()
    }

    fn get_or_create_color_source(
        &self,
        port_id: EntityId,
    ) -> Mutable<Option<Mutable<[u8; 4]>>> {
        self.color_sources
            .borrow_mut()
            .entry(port_id)
            .or_insert_with(|| Mutable::new(None))
            .clone()
    }

    // ----------------------------------------------------------------
    // Dynamic input signal builders
    // ----------------------------------------------------------------

    /// Build a texture input signal: watches upstream if connected, else returns black.
    fn build_tex_input(&self, port_id: EntityId) -> BoxSignal<Rc<TextureBuffer>> {
        let source = self.get_or_create_tex_source(port_id);
        let black = Rc::new(TextureBuffer::new());
        Box::pin(source.signal_cloned().switch(move |opt| -> BoxSignal<Rc<TextureBuffer>> {
            match opt {
                Some(upstream) => Box::pin(upstream.signal_cloned()),
                None => Box::pin(always(black.clone())),
            }
        }))
    }

    /// Build a color input signal: watches upstream if connected, else watches param fallback.
    fn build_color_input(
        &self,
        port_id: EntityId,
        type_id: &str,
        label: &str,
    ) -> BoxSignal<[u8; 4]> {
        let source = self.get_or_create_color_source(port_id);
        let default = params::default_color(type_id, label);
        let param = self.params.get_color(port_id, default);
        Box::pin(source.signal_cloned().switch(move |opt| -> BoxSignal<[u8; 4]> {
            match opt {
                Some(upstream) => Box::pin(upstream.signal()),
                None => Box::pin(param.signal()),
            }
        }))
    }

    /// Build a float input signal: watches param Mutable directly.
    /// (Float ports are never connected from upstream in the texture generator.)
    fn build_float_input(&self, port_id: EntityId, type_id: &str, label: &str) -> BoxSignal<f64> {
        let default = params::default_float(type_id, label);
        let param = self.params.get_float(port_id, default);
        Box::pin(param.signal())
    }

    // ----------------------------------------------------------------
    // Texture signal for canvas preview binding
    // ----------------------------------------------------------------

    /// Get a signal for a node's texture, suitable for canvas rendering.
    /// For non-sink nodes: watches the output port.
    /// For sink nodes: watches the input port's connection.
    pub fn texture_signal_for_node(
        &self,
        node_id: EntityId,
        type_id: &str,
    ) -> BoxSignal<Rc<TextureBuffer>> {
        let is_sink = matches!(type_id, "preview" | "tiled_preview" | "iso_preview");

        if is_sink {
            // Sink: watch input port connection
            let input_port = self.gs.with_graph(|g| {
                g.node_ports(node_id)
                    .iter()
                    .find(|&&pid| {
                        g.world.get::<PortDirection>(pid).copied() == Some(PortDirection::Input)
                    })
                    .copied()
            });
            match input_port {
                Some(pid) => self.build_tex_input(pid),
                None => Box::pin(always(Rc::new(TextureBuffer::new()))),
            }
        } else {
            // Non-sink: watch output port Mutable
            let output_port = self.gs.with_graph(|g| {
                g.node_ports(node_id)
                    .iter()
                    .find(|&&pid| {
                        g.world.get::<PortDirection>(pid).copied() == Some(PortDirection::Output)
                            && g.world
                                .get::<PortSocketType>(pid)
                                .map(|s| s.0)
                                == Some(SocketType::Image)
                    })
                    .copied()
            });
            match output_port {
                Some(pid) => {
                    let m = self.get_or_create_tex_output(pid);
                    Box::pin(m.signal_cloned())
                }
                None => Box::pin(always(Rc::new(TextureBuffer::new()))),
            }
        }
    }

    // ----------------------------------------------------------------
    // Connection handling
    // ----------------------------------------------------------------

    pub fn handle_connect(&self, src_port: EntityId, tgt_port: EntityId, conn_id: EntityId) {
        self.conn_targets.borrow_mut().insert(conn_id, tgt_port);

        let tgt_type = self.gs.with_graph(|g| {
            g.world.get::<PortSocketType>(tgt_port).map(|s| s.0)
        });

        match tgt_type {
            Some(SocketType::Image) => {
                let src = self.tex_outputs.borrow().get(&src_port).cloned();
                if let Some(src_m) = src {
                    if let Some(tgt_source) = self.tex_sources.borrow().get(&tgt_port).cloned() {
                        tgt_source.set(Some(src_m));
                    }
                }
            }
            Some(SocketType::Color) => {
                let src = self.color_outputs.borrow().get(&src_port).cloned();
                if let Some(src_m) = src {
                    if let Some(tgt_source) = self.color_sources.borrow().get(&tgt_port).cloned()
                    {
                        tgt_source.set(Some(src_m));
                    }
                }
            }
            _ => {}
        }
    }

    pub fn handle_disconnect(&self, conn_id: EntityId) {
        let tgt_port = match self.conn_targets.borrow_mut().remove(&conn_id) {
            Some(p) => p,
            None => return,
        };

        if let Some(s) = self.tex_sources.borrow().get(&tgt_port).cloned() {
            s.set(None);
        }
        if let Some(s) = self.color_sources.borrow().get(&tgt_port).cloned() {
            s.set(None);
        }
    }

    // ----------------------------------------------------------------
    // Node setup / teardown
    // ----------------------------------------------------------------

    pub fn setup_node(&self, node_id: EntityId) {
        // Skip if already registered
        if self.node_alive.borrow().contains_key(&node_id) {
            return;
        }

        let type_id = self.gs.with_graph(|g| {
            g.world
                .get::<NodeTypeId>(node_id)
                .map(|t| t.0.clone())
                .unwrap_or_default()
        });

        // Collect port info while holding the graph borrow
        let ports: Vec<(EntityId, PortDirection, SocketType, String)> = self.gs.with_graph(|g| {
            g.node_ports(node_id)
                .iter()
                .map(|&pid| {
                    let dir = g
                        .world
                        .get::<PortDirection>(pid)
                        .copied()
                        .unwrap_or(PortDirection::Input);
                    let stype = g
                        .world
                        .get::<PortSocketType>(pid)
                        .map(|s| s.0)
                        .unwrap_or(SocketType::Float);
                    let label = g
                        .world
                        .get::<PortLabel>(pid)
                        .map(|l| l.0.clone())
                        .unwrap_or_default();
                    (pid, dir, stype, label)
                })
                .collect()
        });

        let alive = Rc::new(Cell::new(true));
        self.node_alive
            .borrow_mut()
            .insert(node_id, alive.clone());

        // Create source selectors for input ports
        for &(pid, dir, stype, _) in &ports {
            if dir != PortDirection::Input {
                continue;
            }
            match stype {
                SocketType::Image => {
                    self.get_or_create_tex_source(pid);
                }
                SocketType::Color => {
                    self.get_or_create_color_source(pid);
                }
                _ => {}
            }
        }

        // Create output Mutables for output ports
        for &(pid, dir, stype, _) in &ports {
            if dir != PortDirection::Output {
                continue;
            }
            match stype {
                SocketType::Image => {
                    self.get_or_create_tex_output(pid);
                }
                SocketType::Color => {
                    self.get_or_create_color_output(pid);
                }
                _ => {}
            }
        }

        // Dispatch to type-specific computation spawner
        match type_id.as_str() {
            "solid_color" => self.spawn_solid_color(node_id, &ports, alive),
            "checker" => self.spawn_checker(node_id, &ports, alive),
            "noise" => self.spawn_noise(node_id, &ports, alive),
            "gradient" => self.spawn_gradient(node_id, &ports, alive),
            "brick" => self.spawn_brick(node_id, &ports, alive),
            "mix" => self.spawn_mix(node_id, &ports, alive),
            "brightness_contrast" => self.spawn_brightness_contrast(node_id, &ports, alive),
            "threshold" => self.spawn_threshold(node_id, &ports, alive),
            "invert" => self.spawn_invert(node_id, &ports, alive),
            "colorize" => self.spawn_colorize(node_id, &ports, alive),
            // Sink nodes don't need computation — canvas rendering is handled by preview.rs
            "preview" | "tiled_preview" | "iso_preview" => {}
            _ => {
                // Unknown type or group node: check for SubgraphRoot
                let is_group = self.gs.with_graph(|g| {
                    g.world.get::<SubgraphRoot>(node_id).is_some()
                });
                if is_group {
                    self.spawn_group(node_id, &ports, alive);
                }
            }
        }
    }

    fn teardown_node(&self, node_id: EntityId) {
        if let Some(alive) = self.node_alive.borrow_mut().remove(&node_id) {
            alive.set(false);
        }
        // Note: we don't remove output/source Mutables here because downstream
        // nodes might still reference them. They become inert (never updated)
        // and get replaced during reconciliation when the downstream is re-wired.
    }

    // ----------------------------------------------------------------
    // Reconciliation (for undo/redo/delete/group)
    // ----------------------------------------------------------------

    pub fn reconcile(&self) {
        let live_nodes: Vec<EntityId> = self
            .gs
            .node_list
            .lock_ref()
            .iter()
            .copied()
            .collect();
        let live_conns: Vec<EntityId> = self
            .gs
            .connection_list
            .lock_ref()
            .iter()
            .copied()
            .collect();

        let live_node_set: HashSet<EntityId> = live_nodes.iter().copied().collect();
        let live_conn_set: HashSet<EntityId> = live_conns.iter().copied().collect();

        // Teardown nodes that no longer exist
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

        // Setup nodes that are new
        for &nid in &live_nodes {
            self.setup_node(nid);
        }

        // Teardown stale connections
        let stale_conns: Vec<EntityId> = self
            .conn_targets
            .borrow()
            .keys()
            .filter(|id| !live_conn_set.contains(id))
            .copied()
            .collect();
        for conn_id in stale_conns {
            self.handle_disconnect(conn_id);
        }

        // Ensure all live connections are wired
        let known_conns: HashSet<EntityId> =
            self.conn_targets.borrow().keys().copied().collect();
        for &conn_id in &live_conns {
            if known_conns.contains(&conn_id) {
                continue;
            }
            let endpoints = self.gs.with_graph(|g| {
                g.world.get::<ConnectionEndpoints>(conn_id).cloned()
            });
            if let Some(ep) = endpoints {
                self.handle_connect(ep.source_port, ep.target_port, conn_id);
            }
        }
    }

    /// Initial setup: scan the current graph and wire everything up.
    pub fn initial_setup(&self) {
        self.reconcile();
    }

    /// Start a watcher that reconciles the reactive graph when node_list or
    /// connection_list changes (handles undo/redo/delete/group).
    pub fn spawn_reconciliation_watcher(self: &Rc<Self>) {
        let reval = self.clone();
        wasm_bindgen_futures::spawn_local(async move {
            map_ref! {
                let _nodes = reval.gs.node_list.signal_vec_cloned().to_signal_cloned(),
                let _conns = reval.gs.connection_list.signal_vec_cloned().to_signal_cloned()
                => {}
            }
            .for_each(move |_| {
                reval.reconcile();
                async {}
            })
            .await;
        });
    }

    // ----------------------------------------------------------------
    // Per-node-type computation spawners
    // ----------------------------------------------------------------

    fn find_port(
        ports: &[(EntityId, PortDirection, SocketType, String)],
        dir: PortDirection,
        label: &str,
    ) -> Option<EntityId> {
        ports
            .iter()
            .find(|(_, d, _, l)| *d == dir && l == label)
            .map(|(id, _, _, _)| *id)
    }

    fn find_output_image(
        ports: &[(EntityId, PortDirection, SocketType, String)],
    ) -> Option<EntityId> {
        ports
            .iter()
            .find(|(_, d, s, _)| *d == PortDirection::Output && *s == SocketType::Image)
            .map(|(id, _, _, _)| *id)
    }

    fn find_output_color(
        ports: &[(EntityId, PortDirection, SocketType, String)],
    ) -> Option<EntityId> {
        ports
            .iter()
            .find(|(_, d, s, _)| *d == PortDirection::Output && *s == SocketType::Color)
            .map(|(id, _, _, _)| *id)
    }

    fn spawn_solid_color(
        &self,
        _node_id: EntityId,
        ports: &[(EntityId, PortDirection, SocketType, String)],
        alive: Rc<Cell<bool>>,
    ) {
        let out_port = match Self::find_output_color(ports) {
            Some(p) => p,
            None => return,
        };
        let output = self.get_or_create_color_output(out_port);
        let param = self
            .params
            .get_color(out_port, params::default_color("solid_color", "Color"));

        wasm_bindgen_futures::spawn_local(async move {
            param
                .signal()
                .for_each(move |color| {
                    if alive.get() {
                        output.set(color);
                    }
                    async {}
                })
                .await;
        });
    }

    fn spawn_checker(
        &self,
        _node_id: EntityId,
        ports: &[(EntityId, PortDirection, SocketType, String)],
        alive: Rc<Cell<bool>>,
    ) {
        let ca_port = Self::find_port(ports, PortDirection::Input, "Color A");
        let cb_port = Self::find_port(ports, PortDirection::Input, "Color B");
        let size_port = Self::find_port(ports, PortDirection::Input, "Size");
        let out_port = Self::find_output_image(ports);

        let (ca_port, cb_port, size_port, out_port) =
            match (ca_port, cb_port, size_port, out_port) {
                (Some(a), Some(b), Some(s), Some(o)) => (a, b, s, o),
                _ => return,
            };

        let ca_sig = self.build_color_input(ca_port, "checker", "Color A");
        let cb_sig = self.build_color_input(cb_port, "checker", "Color B");
        let size_sig = self.build_float_input(size_port, "checker", "Size");
        let output = self.get_or_create_tex_output(out_port);

        wasm_bindgen_futures::spawn_local(async move {
            map_ref! {
                let ca = ca_sig,
                let cb = cb_sig,
                let size = size_sig => { (*ca, *cb, *size) }
            }
            .for_each(move |(ca, cb, size)| {
                if alive.get() {
                    output.set(Rc::new(eval::eval_checker(ca, cb, size)));
                }
                async {}
            })
            .await;
        });
    }

    fn spawn_noise(
        &self,
        _node_id: EntityId,
        ports: &[(EntityId, PortDirection, SocketType, String)],
        alive: Rc<Cell<bool>>,
    ) {
        let scale_port = Self::find_port(ports, PortDirection::Input, "Scale");
        let seed_port = Self::find_port(ports, PortDirection::Input, "Seed");
        let out_port = Self::find_output_image(ports);

        let (scale_port, seed_port, out_port) = match (scale_port, seed_port, out_port) {
            (Some(s), Some(se), Some(o)) => (s, se, o),
            _ => return,
        };

        let scale_sig = self.build_float_input(scale_port, "noise", "Scale");
        let seed_sig = self.build_float_input(seed_port, "noise", "Seed");
        let output = self.get_or_create_tex_output(out_port);

        wasm_bindgen_futures::spawn_local(async move {
            map_ref! {
                let scale = scale_sig,
                let seed = seed_sig => { (*scale, *seed) }
            }
            .for_each(move |(scale, seed)| {
                if alive.get() {
                    output.set(Rc::new(eval::eval_noise(scale, seed)));
                }
                async {}
            })
            .await;
        });
    }

    fn spawn_gradient(
        &self,
        _node_id: EntityId,
        ports: &[(EntityId, PortDirection, SocketType, String)],
        alive: Rc<Cell<bool>>,
    ) {
        let ca_port = Self::find_port(ports, PortDirection::Input, "Color A");
        let cb_port = Self::find_port(ports, PortDirection::Input, "Color B");
        let out_port = Self::find_output_image(ports);

        let (ca_port, cb_port, out_port) = match (ca_port, cb_port, out_port) {
            (Some(a), Some(b), Some(o)) => (a, b, o),
            _ => return,
        };

        let ca_sig = self.build_color_input(ca_port, "gradient", "Color A");
        let cb_sig = self.build_color_input(cb_port, "gradient", "Color B");
        let output = self.get_or_create_tex_output(out_port);

        wasm_bindgen_futures::spawn_local(async move {
            map_ref! {
                let ca = ca_sig,
                let cb = cb_sig => { (*ca, *cb) }
            }
            .for_each(move |(ca, cb)| {
                if alive.get() {
                    output.set(Rc::new(eval::eval_gradient(ca, cb)));
                }
                async {}
            })
            .await;
        });
    }

    fn spawn_brick(
        &self,
        _node_id: EntityId,
        ports: &[(EntityId, PortDirection, SocketType, String)],
        alive: Rc<Cell<bool>>,
    ) {
        let mortar_port = Self::find_port(ports, PortDirection::Input, "Mortar");
        let brick_port = Self::find_port(ports, PortDirection::Input, "Brick");
        let rows_port = Self::find_port(ports, PortDirection::Input, "Rows");
        let out_port = Self::find_output_image(ports);

        let (mortar_port, brick_port, rows_port, out_port) =
            match (mortar_port, brick_port, rows_port, out_port) {
                (Some(m), Some(b), Some(r), Some(o)) => (m, b, r, o),
                _ => return,
            };

        let mortar_sig = self.build_color_input(mortar_port, "brick", "Mortar");
        let brick_sig = self.build_color_input(brick_port, "brick", "Brick");
        let rows_sig = self.build_float_input(rows_port, "brick", "Rows");
        let output = self.get_or_create_tex_output(out_port);

        wasm_bindgen_futures::spawn_local(async move {
            map_ref! {
                let mortar = mortar_sig,
                let brick = brick_sig,
                let rows = rows_sig => { (*mortar, *brick, *rows) }
            }
            .for_each(move |(mortar, brick, rows)| {
                if alive.get() {
                    output.set(Rc::new(eval::eval_brick(mortar, brick, rows)));
                }
                async {}
            })
            .await;
        });
    }

    fn spawn_mix(
        &self,
        _node_id: EntityId,
        ports: &[(EntityId, PortDirection, SocketType, String)],
        alive: Rc<Cell<bool>>,
    ) {
        let a_port = Self::find_port(ports, PortDirection::Input, "A");
        let b_port = Self::find_port(ports, PortDirection::Input, "B");
        let factor_port = Self::find_port(ports, PortDirection::Input, "Factor");
        let out_port = Self::find_output_image(ports);

        let (a_port, b_port, factor_port, out_port) =
            match (a_port, b_port, factor_port, out_port) {
                (Some(a), Some(b), Some(f), Some(o)) => (a, b, f, o),
                _ => return,
            };

        let a_sig = self.build_tex_input(a_port);
        let b_sig = self.build_tex_input(b_port);
        let factor_sig = self.build_float_input(factor_port, "mix", "Factor");
        let output = self.get_or_create_tex_output(out_port);

        wasm_bindgen_futures::spawn_local(async move {
            map_ref! {
                let a = a_sig,
                let b = b_sig,
                let factor = factor_sig => { (a.clone(), b.clone(), *factor) }
            }
            .for_each(move |(a, b, factor)| {
                if alive.get() {
                    output.set(Rc::new(eval::eval_mix(Some(a), Some(b), factor)));
                }
                async {}
            })
            .await;
        });
    }

    fn spawn_brightness_contrast(
        &self,
        _node_id: EntityId,
        ports: &[(EntityId, PortDirection, SocketType, String)],
        alive: Rc<Cell<bool>>,
    ) {
        let tex_port = Self::find_port(ports, PortDirection::Input, "Texture");
        let bright_port = Self::find_port(ports, PortDirection::Input, "Brightness");
        let contrast_port = Self::find_port(ports, PortDirection::Input, "Contrast");
        let out_port = Self::find_output_image(ports);

        let (tex_port, bright_port, contrast_port, out_port) =
            match (tex_port, bright_port, contrast_port, out_port) {
                (Some(t), Some(b), Some(c), Some(o)) => (t, b, c, o),
                _ => return,
            };

        let tex_sig = self.build_tex_input(tex_port);
        let bright_sig = self.build_float_input(bright_port, "brightness_contrast", "Brightness");
        let contrast_sig =
            self.build_float_input(contrast_port, "brightness_contrast", "Contrast");
        let output = self.get_or_create_tex_output(out_port);

        wasm_bindgen_futures::spawn_local(async move {
            map_ref! {
                let tex = tex_sig,
                let brightness = bright_sig,
                let contrast = contrast_sig => { (tex.clone(), *brightness, *contrast) }
            }
            .for_each(move |(tex, brightness, contrast)| {
                if alive.get() {
                    output.set(Rc::new(eval::eval_brightness_contrast(
                        Some(tex),
                        brightness,
                        contrast,
                    )));
                }
                async {}
            })
            .await;
        });
    }

    fn spawn_threshold(
        &self,
        _node_id: EntityId,
        ports: &[(EntityId, PortDirection, SocketType, String)],
        alive: Rc<Cell<bool>>,
    ) {
        let tex_port = Self::find_port(ports, PortDirection::Input, "Texture");
        let level_port = Self::find_port(ports, PortDirection::Input, "Level");
        let out_port = Self::find_output_image(ports);

        let (tex_port, level_port, out_port) = match (tex_port, level_port, out_port) {
            (Some(t), Some(l), Some(o)) => (t, l, o),
            _ => return,
        };

        let tex_sig = self.build_tex_input(tex_port);
        let level_sig = self.build_float_input(level_port, "threshold", "Level");
        let output = self.get_or_create_tex_output(out_port);

        wasm_bindgen_futures::spawn_local(async move {
            map_ref! {
                let tex = tex_sig,
                let level = level_sig => { (tex.clone(), *level) }
            }
            .for_each(move |(tex, level)| {
                if alive.get() {
                    output.set(Rc::new(eval::eval_threshold(Some(tex), level)));
                }
                async {}
            })
            .await;
        });
    }

    fn spawn_invert(
        &self,
        _node_id: EntityId,
        ports: &[(EntityId, PortDirection, SocketType, String)],
        alive: Rc<Cell<bool>>,
    ) {
        let tex_port = Self::find_port(ports, PortDirection::Input, "Texture");
        let out_port = Self::find_output_image(ports);

        let (tex_port, out_port) = match (tex_port, out_port) {
            (Some(t), Some(o)) => (t, o),
            _ => return,
        };

        let tex_sig = self.build_tex_input(tex_port);
        let output = self.get_or_create_tex_output(out_port);

        wasm_bindgen_futures::spawn_local(async move {
            tex_sig
                .for_each(move |tex| {
                    if alive.get() {
                        output.set(Rc::new(eval::eval_invert(Some(tex))));
                    }
                    async {}
                })
                .await;
        });
    }

    fn spawn_colorize(
        &self,
        _node_id: EntityId,
        ports: &[(EntityId, PortDirection, SocketType, String)],
        alive: Rc<Cell<bool>>,
    ) {
        let tex_port = Self::find_port(ports, PortDirection::Input, "Texture");
        let tint_port = Self::find_port(ports, PortDirection::Input, "Tint");
        let out_port = Self::find_output_image(ports);

        let (tex_port, tint_port, out_port) = match (tex_port, tint_port, out_port) {
            (Some(t), Some(ti), Some(o)) => (t, ti, o),
            _ => return,
        };

        let tex_sig = self.build_tex_input(tex_port);
        let tint_sig = self.build_color_input(tint_port, "colorize", "Tint");
        let output = self.get_or_create_tex_output(out_port);

        wasm_bindgen_futures::spawn_local(async move {
            map_ref! {
                let tex = tex_sig,
                let tint = tint_sig => { (tex.clone(), *tint) }
            }
            .for_each(move |(tex, tint)| {
                if alive.get() {
                    output.set(Rc::new(eval::eval_colorize(Some(tex), tint)));
                }
                async {}
            })
            .await;
        });
    }

    fn spawn_group(
        &self,
        node_id: EntityId,
        ports: &[(EntityId, PortDirection, SocketType, String)],
        alive: Rc<Cell<bool>>,
    ) {
        // Group nodes use imperative subgraph evaluation.
        // Build a combined signal from all input ports, then on any change
        // run the topo-sort evaluation and write results to output Mutables.

        // Collect all input signals into a single trigger
        let mut input_sigs: Vec<BoxSignal<()>> = Vec::new();
        for &(pid, dir, stype, _) in ports {
            if dir != PortDirection::Input {
                continue;
            }
            match stype {
                SocketType::Image => {
                    let sig = self.build_tex_input(pid);
                    input_sigs.push(Box::pin(sig.map(|_| ())));
                }
                SocketType::Color => {
                    let label = ports
                        .iter()
                        .find(|(id, _, _, _)| *id == pid)
                        .map(|(_, _, _, l)| l.as_str())
                        .unwrap_or("");
                    let sig = self.build_color_input(pid, "", label);
                    input_sigs.push(Box::pin(sig.map(|_| ())));
                }
                SocketType::Float => {
                    let label = ports
                        .iter()
                        .find(|(id, _, _, _)| *id == pid)
                        .map(|(_, _, _, l)| l.as_str())
                        .unwrap_or("");
                    let sig = self.build_float_input(pid, "", label);
                    input_sigs.push(Box::pin(sig.map(|_| ())));
                }
                _ => {}
            }
        }

        // Collect output port IDs
        let tex_out_ports: Vec<EntityId> = ports
            .iter()
            .filter(|(_, d, s, _)| *d == PortDirection::Output && *s == SocketType::Image)
            .map(|(id, _, _, _)| *id)
            .collect();
        let color_out_ports: Vec<EntityId> = ports
            .iter()
            .filter(|(_, d, s, _)| *d == PortDirection::Output && *s == SocketType::Color)
            .map(|(id, _, _, _)| *id)
            .collect();

        let tex_outputs: HashMap<EntityId, Mutable<Rc<TextureBuffer>>> = tex_out_ports
            .iter()
            .map(|&pid| (pid, self.get_or_create_tex_output(pid)))
            .collect();
        let color_outputs: HashMap<EntityId, Mutable<[u8; 4]>> = color_out_ports
            .iter()
            .map(|&pid| (pid, self.get_or_create_color_output(pid)))
            .collect();

        let gs = self.gs.clone();
        let params = self.params.clone();

        // Use a version counter that all input signals bump
        let version = Mutable::new(0u64);
        for sig in input_sigs {
            let version = version.clone();
            wasm_bindgen_futures::spawn_local(async move {
                sig.for_each(move |_| {
                    version.set(version.get().wrapping_add(1));
                    async {}
                })
                .await;
            });
        }

        // Also watch internal subgraph param signals so edits inside the group
        // trigger recomputation of group outputs.
        {
            let subgraph_id = self.gs.with_graph(|g| {
                g.world.get::<SubgraphRoot>(node_id).map(|s| s.0)
            });
            if let Some(sub_id) = subgraph_id {
                let internal_ports: Vec<(EntityId, SocketType)> = {
                    let editor = self.gs.editor.borrow();
                    if let Some(sub) = editor.graph(sub_id) {
                        sub.world.query::<nodegraph_core::graph::node::NodeHeader>()
                            .flat_map(|(nid, _)| {
                                sub.node_ports(nid).iter().filter_map(|&pid| {
                                    let st = sub.world.get::<PortSocketType>(pid).map(|s| s.0)?;
                                    Some((pid, st))
                                }).collect::<Vec<_>>()
                            })
                            .collect()
                    } else {
                        Vec::new()
                    }
                };
                for (pid, st) in internal_ports {
                    let version = version.clone();
                    match st {
                        SocketType::Float => {
                            let m = self.params.get_float(pid, crate::params::default_float("", ""));
                            wasm_bindgen_futures::spawn_local(async move {
                                m.signal().for_each(move |_| {
                                    version.set(version.get().wrapping_add(1));
                                    async {}
                                }).await;
                            });
                        }
                        SocketType::Color => {
                            let m = self.params.get_color(pid, crate::params::default_color("", ""));
                            wasm_bindgen_futures::spawn_local(async move {
                                m.signal().for_each(move |_| {
                                    version.set(version.get().wrapping_add(1));
                                    async {}
                                }).await;
                            });
                        }
                        _ => {}
                    }
                }
            }
        }

        // Also bump version when the connection list changes (handles reconnects inside the group)
        {
            let version = version.clone();
            let gs2 = self.gs.clone();
            wasm_bindgen_futures::spawn_local(async move {
                gs2.connection_list.signal_vec_cloned().to_signal_cloned()
                    .for_each(move |_| {
                        version.set(version.get().wrapping_add(1));
                        async {}
                    }).await;
            });
        }

        wasm_bindgen_futures::spawn_local(async move {
            version
                .signal()
                .for_each(move |_| {
                    if alive.get() {
                        let snap = params.snapshot();
                        let result = {
                            let editor = gs.editor.borrow();
                            eval::evaluate(&editor, &snap)
                        };
                        for (&pid, m) in &tex_outputs {
                            if let Some(tex) = result.textures.get(&pid) {
                                m.set(tex.clone());
                            }
                        }
                        for (&pid, m) in &color_outputs {
                            if let Some(&color) = result.colors.get(&pid) {
                                m.set(color);
                            }
                        }
                    }
                    async {}
                })
                .await;
        });
    }
}
