use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use wasm_bindgen::JsCast;
use futures_signals::signal::Mutable;
use futures_signals::signal_vec::MutableVec;

use nodegraph_core::commands::UndoHistory;
use nodegraph_core::graph::{GraphEditor, NodeGraph, GroupIOKind};
use nodegraph_core::graph::node::{NodeHeader, NodePosition, MuteState};
use nodegraph_core::graph::port::PortDirection;
use nodegraph_core::graph::connection::ConnectionEndpoints;
use nodegraph_core::interaction::{InteractionController, InputEvent, SideEffect, InteractionState};
use nodegraph_core::layout::{self, BezierPath, Vec2, PORT_RADIUS};
use nodegraph_core::search::NodeTypeRegistry;
use nodegraph_core::store::EntityId;
use nodegraph_core::interaction::HitTarget;
use nodegraph_core::types::socket_type::SocketType;

use dominator::Dom;
use crate::theme::Theme;

/// User-provided callback to render custom content inside a node body.
/// Returns `Some(Dom)` to add content below port rows, or `None` to skip.
pub type CustomNodeBodyFn = dyn Fn(EntityId, &Rc<GraphSignals>) -> Option<Dom>;

/// User-provided callback to render inline widgets on port rows.
/// Args: (node_id, port_id, socket_type, port_direction, node_type_id, is_connected, gs).
/// Returns `Some(Dom)` to insert a widget after the port label, or `None` to skip.
pub type PortWidgetFn = dyn Fn(EntityId, EntityId, SocketType, PortDirection, &str, bool, &Rc<GraphSignals>) -> Option<Dom>;

pub const ATTR_NODE_ID: &str = "data-node-id";
pub const ATTR_PORT_ID: &str = "data-port-id";
pub const ATTR_VIEWPORT_INNER: &str = "data-viewport-inner";

pub fn is_valid_connection_target(
    from_output: bool, src_type: SocketType, tgt_dir: PortDirection, tgt_type: SocketType,
) -> bool {
    let dir_ok = if from_output { tgt_dir == PortDirection::Input } else { tgt_dir == PortDirection::Output };
    dir_ok && src_type.is_compatible_with(&tgt_type)
}

pub fn is_exact_type_match(a: SocketType, b: SocketType) -> bool { a == b }

pub struct GraphSignals {
    pub editor: Rc<RefCell<GraphEditor>>,
    pub history: Rc<RefCell<UndoHistory>>,
    pub controller: Rc<RefCell<InteractionController>>,

    pub node_list: MutableVec<EntityId>,
    pub connection_list: MutableVec<EntityId>,
    pub frame_list: MutableVec<EntityId>,
    pub node_positions: Rc<RefCell<HashMap<EntityId, Mutable<(f64, f64)>>>>,
    pub node_headers: Rc<RefCell<HashMap<EntityId, Mutable<NodeHeader>>>>,
    pub frame_bounds: Rc<RefCell<HashMap<EntityId, Mutable<(f64, f64, f64, f64)>>>>,

    pub selection: Mutable<Vec<EntityId>>,
    pub selected_frames: Mutable<Vec<EntityId>>,
    pub pan: Mutable<(f64, f64)>,
    pub zoom: Mutable<f64>,

    pub connecting_from: Mutable<Option<(EntityId, SocketType, bool)>>,
    pub drop_target_connection: Mutable<Option<EntityId>>,
    pub drop_target_port: Mutable<Option<EntityId>>,
    pub preview_wire: Mutable<Option<BezierPath>>,
    pub box_select_rect: Mutable<Option<(f64, f64, f64, f64)>>,
    pub cut_line_points: Mutable<Vec<(f64, f64)>>,
    pub cursor_world: Mutable<(f64, f64)>,

    pub registry: Rc<RefCell<NodeTypeRegistry>>,
    pub search_menu: Mutable<Option<(f64, f64)>>,
    pub pending_connection: Mutable<Option<(EntityId, SocketType, bool)>>,

    pub current_graph_id: Mutable<EntityId>,
    last_synced_graph: std::cell::Cell<Option<EntityId>>,
    pub breadcrumb: MutableVec<(EntityId, String)>,

    pub theme: Rc<Theme>,
    pub custom_node_body: Rc<RefCell<Option<Rc<CustomNodeBodyFn>>>>,
    pub port_widget: Rc<RefCell<Option<Rc<PortWidgetFn>>>>,
    pub graph_bounds: Mutable<(f64, f64, f64, f64)>,
    pub viewport_size: Mutable<(f64, f64)>,

    // Context menu
    pub context_menu: Mutable<Option<(HitTarget, f64, f64)>>,

    // Event callbacks
    pub on_connect: RefCell<Option<Box<dyn Fn(EntityId, EntityId, EntityId)>>>,
    pub on_disconnect: RefCell<Option<Box<dyn Fn(EntityId)>>>,
    pub on_selection_changed: RefCell<Option<Box<dyn Fn(&[EntityId])>>>,
    pub on_node_moved: RefCell<Option<Box<dyn Fn(&[(EntityId, f64, f64)])>>>,
}

impl GraphSignals {
    pub fn new() -> Rc<Self> {
        let editor = GraphEditor::new();
        let root_id = editor.root_graph_id();
        Rc::new(Self {
            editor: Rc::new(RefCell::new(editor)),
            history: Rc::new(RefCell::new(UndoHistory::new())),
            controller: Rc::new(RefCell::new(InteractionController::new())),
            node_list: MutableVec::new(),
            connection_list: MutableVec::new(),
            frame_list: MutableVec::new(),
            node_positions: Rc::new(RefCell::new(HashMap::new())),
            node_headers: Rc::new(RefCell::new(HashMap::new())),
            frame_bounds: Rc::new(RefCell::new(HashMap::new())),
            selection: Mutable::new(Vec::new()),
            selected_frames: Mutable::new(Vec::new()),
            pan: Mutable::new((0.0, 0.0)),
            zoom: Mutable::new(1.0),
            connecting_from: Mutable::new(None),
            drop_target_connection: Mutable::new(None),
            drop_target_port: Mutable::new(None),
            preview_wire: Mutable::new(None),
            box_select_rect: Mutable::new(None),
            cut_line_points: Mutable::new(Vec::new()),
            cursor_world: Mutable::new((0.0, 0.0)),
            registry: Rc::new(RefCell::new(NodeTypeRegistry::new())),
            search_menu: Mutable::new(None),
            pending_connection: Mutable::new(None),
            current_graph_id: Mutable::new(root_id),
            breadcrumb: MutableVec::new_with_values(vec![(root_id, "Root".to_string())]),
            last_synced_graph: std::cell::Cell::new(Some(root_id)),
            theme: Theme::dark(),
            custom_node_body: Rc::new(RefCell::new(None)),
            port_widget: Rc::new(RefCell::new(None)),
            graph_bounds: Mutable::new((0.0, 0.0, 800.0, 600.0)),
            viewport_size: Mutable::new((800.0, 600.0)),
            context_menu: Mutable::new(None),
            on_connect: RefCell::new(None),
            on_disconnect: RefCell::new(None),
            on_selection_changed: RefCell::new(None),
            on_node_moved: RefCell::new(None),
        })
    }

    // ============================================================
    // Convenience accessors
    // ============================================================

    pub fn with_graph<R>(&self, f: impl FnOnce(&NodeGraph) -> R) -> R {
        let editor = self.editor.borrow();
        f(editor.current_graph())
    }

    pub fn with_graph_mut<R>(&self, f: impl FnOnce(&mut NodeGraph) -> R) -> R {
        let mut editor = self.editor.borrow_mut();
        f(editor.current_graph_mut())
    }

    pub fn node_count(&self) -> usize { self.with_graph(|g| g.node_count()) }
    pub fn connection_count(&self) -> usize { self.with_graph(|g| g.connection_count()) }

    pub fn select_single(&self, node_id: EntityId) {
        self.controller.borrow_mut().selection.clear();
        self.controller.borrow_mut().selection.select(node_id);
        self.sync_selection();
    }

    /// Snapshot the current editor state for undo. Call BEFORE mutating.
    pub fn save_undo(&self) {
        let editor = self.editor.borrow();
        self.history.borrow_mut().save(&editor);
    }

    // ============================================================
    // Node/connection operations (all undoable via snapshot)
    // ============================================================

    pub fn add_node(&self, title: &str, position: (f64, f64), ports: Vec<(PortDirection, SocketType, String)>) -> (EntityId, Vec<EntityId>) {
        self.add_node_typed(title, None, position, ports)
    }

    pub fn add_node_typed(&self, title: &str, type_id: Option<&str>, position: (f64, f64), ports: Vec<(PortDirection, SocketType, String)>) -> (EntityId, Vec<EntityId>) {
        let (node_id, port_ids) = self.with_graph_mut(|graph| {
            let nid = graph.add_node(title, position);
            let pids: Vec<EntityId> = ports.iter()
                .map(|(dir, st, label)| graph.add_port(nid, *dir, *st, label))
                .collect();
            if let Some(tid) = type_id {
                graph.world.insert(nid, nodegraph_core::graph::node::NodeTypeId(tid.to_string()));
            }
            (nid, pids)
        });
        let header = self.with_graph(|g| g.world.get::<NodeHeader>(node_id).cloned()
            .unwrap_or(NodeHeader { title: title.to_string(), color: [100,100,100], collapsed: false }));
        self.node_positions.borrow_mut().insert(node_id, Mutable::new(position));
        self.node_headers.borrow_mut().insert(node_id, Mutable::new(header));
        self.node_list.lock_mut().push_cloned(node_id);
        (node_id, port_ids)
    }

    pub fn spawn_from_registry(&self, type_id: &str, position: (f64, f64)) {
        // Group IO nodes bypass the registry — they require parent graph context
        if type_id == "group_input" || type_id == "group_output" {
            let kind = if type_id == "group_input" { GroupIOKind::Input } else { GroupIOKind::Output };
            self.save_undo();
            self.create_group_io_node(kind, position);
            self.pending_connection.set(None);
            self.search_menu.set(None);
            self.full_sync();
            return;
        }

        let def = match self.registry.borrow().get(type_id) { Some(d) => d.clone(), None => return };
        let mut all_ports: Vec<(PortDirection, SocketType, String)> = Vec::new();
        for p in &def.input_ports { all_ports.push((p.direction, p.socket_type, p.label.clone())); }
        for p in &def.output_ports { all_ports.push((p.direction, p.socket_type, p.label.clone())); }

        self.save_undo();
        let type_id_owned = def.type_id.clone();
        let (node_id, new_ports) = self.add_node(&def.display_name, position, all_ports);

        // Store type_id on the node for type dispatch
        self.with_graph_mut(|g| {
            g.world.insert(node_id, nodegraph_core::graph::node::NodeTypeId(type_id_owned.clone()));
        });

        // Mark reroute nodes
        if type_id_owned == "reroute" {
            self.with_graph_mut(|g| {
                g.world.insert(node_id, nodegraph_core::graph::reroute::IsReroute);
            });
        }

        if let Some((src_port, src_type, from_output)) = self.pending_connection.get() {
            for &pid in &new_ports {
                let info = self.with_graph(|g| {
                    (g.world.get::<PortDirection>(pid).copied(),
                     g.world.get::<nodegraph_core::graph::port::PortSocketType>(pid).map(|s| s.0))
                });
                if let (Some(dir), Some(st)) = info {
                    if is_valid_connection_target(from_output, src_type, dir, st) {
                        self.connect_ports_no_undo(src_port, pid);
                        break;
                    }
                }
            }
        }
        self.pending_connection.set(None);
        self.search_menu.set(None);
    }

    pub fn connect_ports(&self, source: EntityId, target: EntityId) -> Result<EntityId, nodegraph_core::graph::ConnectionError> {
        self.save_undo();
        self.connect_ports_inner(source, target)
    }

    fn connect_ports_no_undo(&self, source: EntityId, target: EntityId) -> Option<EntityId> {
        self.connect_ports_inner(source, target).ok()
    }

    fn connect_ports_inner(&self, source: EntityId, target: EntityId) -> Result<EntityId, nodegraph_core::graph::ConnectionError> {
        let conn_id = self.with_graph_mut(|g| g.connect(source, target))?;
        self.connection_list.lock_mut().push_cloned(conn_id);
        self.reconcile_connections();
        self.adapt_io_ports_after_connect(source, target);
        if let Some(cb) = self.on_connect.borrow().as_ref() { cb(source, target, conn_id); }
        Ok(conn_id)
    }

    pub fn port_world_pos(&self, port_id: EntityId) -> Option<Vec2> {
        self.with_graph(|g| layout::compute_port_world_position(g, port_id))
    }

    fn find_port_at(&self, world: Vec2) -> Option<EntityId> {
        let editor = self.editor.borrow();
        let graph = editor.current_graph();
        let cache = layout::LayoutCache::compute(graph);
        let hit_radius = PORT_RADIUS + 5.0;
        let mut best: Option<(EntityId, f64)> = None;
        for (_, nl) in &cache.layouts {
            for &(pid, pos) in nl.input_port_positions.iter().chain(nl.output_port_positions.iter()) {
                let dist = world.distance_to(pos);
                if dist <= hit_radius && best.map_or(true, |(_, d)| dist < d) {
                    best = Some((pid, dist));
                }
            }
        }
        best.map(|(id, _)| id)
    }

    fn adapt_io_ports_after_connect(&self, port_a: EntityId, port_b: EntityId) {
        let (a_type, b_type, a_is_io, b_is_io) = self.with_graph(|g| {
            let at = g.world.get::<nodegraph_core::graph::port::PortSocketType>(port_a).map(|s| s.0);
            let bt = g.world.get::<nodegraph_core::graph::port::PortSocketType>(port_b).map(|s| s.0);
            let a_io = g.world.get::<nodegraph_core::graph::port::PortOwner>(port_a)
                .and_then(|o| g.world.get::<GroupIOKind>(o.0)).is_some();
            let b_io = g.world.get::<nodegraph_core::graph::port::PortOwner>(port_b)
                .and_then(|o| g.world.get::<GroupIOKind>(o.0)).is_some();
            (at, bt, a_io, b_io)
        });
        let mut editor = self.editor.borrow_mut();
        if a_is_io { if let Some(bt) = b_type { editor.adapt_group_io_port(port_a, bt); } }
        if b_is_io { if let Some(at) = a_type { editor.adapt_group_io_port(port_b, at); } }
    }

    // ============================================================
    // Search menu
    // ============================================================

    pub fn open_search_menu(&self, world_x: f64, world_y: f64) {
        self.pending_connection.set(None);
        self.search_menu.set(Some((world_x, world_y)));
        wasm_bindgen_futures::spawn_local(async {
            let promise = js_sys::Promise::resolve(&wasm_bindgen::JsValue::NULL);
            let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
            if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
                if let Some(el) = doc.query_selector("[data-search-menu] input").ok().flatten() {
                    if let Ok(html_el) = el.dyn_into::<web_sys::HtmlElement>() { let _ = html_el.focus(); }
                }
            }
        });
    }

    pub fn close_search_menu(&self) {
        self.search_menu.set(None);
        self.pending_connection.set(None);
    }

    // ============================================================
    // Connecting (drag-to-connect)
    // ============================================================

    pub fn start_connecting(self: &Rc<Self>, port_id: EntityId, _screen: Vec2, world: Vec2) {
        let (from_output, socket_type) = self.with_graph(|g| {
            let fo = g.world.get::<PortDirection>(port_id).map(|d| *d == PortDirection::Output).unwrap_or(false);
            let st = g.world.get::<nodegraph_core::graph::port::PortSocketType>(port_id).map(|s| s.0).unwrap_or(SocketType::Float);
            (fo, st)
        });
        self.connecting_from.set(Some((port_id, socket_type, from_output)));
        self.controller.borrow_mut().state = InteractionState::ConnectingPort {
            source_port: port_id, from_output, cursor_world: world,
        };
    }

    // ============================================================
    // Input handling
    // ============================================================

    pub fn handle_input(self: &Rc<Self>, event: InputEvent) {
        let is_connecting = matches!(self.controller.borrow().state, InteractionState::ConnectingPort { .. });

        if is_connecting {
            if let InputEvent::MouseUp { .. } = &event {
                let source_port = match &self.controller.borrow().state {
                    InteractionState::ConnectingPort { source_port, .. } => *source_port,
                    _ => unreachable!(),
                };
                if let Some(target_port) = self.drop_target_port.get() {
                    self.save_undo();
                    let result = self.with_graph_mut(|g| g.connect(source_port, target_port));
                    if let Ok(conn_id) = result {
                        self.connection_list.lock_mut().push_cloned(conn_id);
                        self.reconcile_connections();
                        self.adapt_io_ports_after_connect(source_port, target_port);
                        if let Some(cb) = self.on_connect.borrow().as_ref() { cb(source_port, target_port, conn_id); }
                    }
                    self.connecting_from.set(None);
                } else {
                    let world = match &event { InputEvent::MouseUp { world, .. } => *world, _ => Vec2::new(0.0, 0.0) };
                    // Use connecting_from if set (from start_connecting), otherwise derive from controller state
                    let cf = self.connecting_from.get().or_else(|| {
                        let (from_output, _) = match &self.controller.borrow().state {
                            InteractionState::ConnectingPort { from_output, .. } => (*from_output, source_port),
                            _ => return None,
                        };
                        let st = self.with_graph(|g| {
                            g.world.get::<nodegraph_core::graph::port::PortSocketType>(source_port).map(|s| s.0)
                        }).unwrap_or(SocketType::Float);
                        Some((source_port, st, from_output))
                    });
                    if let Some(cf) = cf {
                        self.pending_connection.set(Some(cf));
                        self.search_menu.set(Some((world.x, world.y)));
                    }
                    self.connecting_from.set(None);
                }
                self.controller.borrow_mut().state = InteractionState::Idle;
                self.preview_wire.set(None);
                self.drop_target_port.set(None);
                return;
            }
            if let InputEvent::MouseMove { world, .. } = &event {
                if let Some((source_port, src_type, from_output)) = self.connecting_from.get() {
                    if let Some(src_pos) = self.port_world_pos(source_port) {
                        self.preview_wire.set(Some(layout::compute_preview_path(src_pos, *world, from_output)));
                    }
                    let target = self.find_port_at(*world).and_then(|pid| {
                        if pid == source_port { return None; }
                        self.with_graph(|g| {
                            let tgt_dir = g.world.get::<PortDirection>(pid).copied()?;
                            let tgt_type = g.world.get::<nodegraph_core::graph::port::PortSocketType>(pid)?.0;
                            if is_valid_connection_target(from_output, src_type, tgt_dir, tgt_type) { Some(pid) } else { None }
                        })
                    });
                    self.drop_target_port.set(target);
                }
                return;
            }
        }

        // Track drag state transitions for undo + reroute insert
        let was_idle = matches!(self.controller.borrow().state, InteractionState::Idle);
        let was_dragging = matches!(self.controller.borrow().state, InteractionState::DraggingNodes { .. });
        let dragged_nodes: Vec<EntityId> = if was_dragging {
            match &self.controller.borrow().state {
                InteractionState::DraggingNodes { node_ids, .. } => node_ids.clone(),
                _ => Vec::new(),
            }
        } else { Vec::new() };

        let effects = {
            let mut editor = self.editor.borrow_mut();
            let mut ctrl = self.controller.borrow_mut();
            ctrl.handle_event(event, editor.current_graph_mut())
        };

        let is_now_dragging = matches!(self.controller.borrow().state, InteractionState::DraggingNodes { .. });
        let is_now_idle = matches!(self.controller.borrow().state, InteractionState::Idle);

        // Save undo at the START of a drag (transition from Idle to DraggingNodes)
        if was_idle && is_now_dragging {
            self.save_undo();
        }

        // Node-on-wire insert: if a node was dragged onto a connection, auto-insert it
        if was_dragging && is_now_idle && dragged_nodes.len() == 1 {
            let node_id = dragged_nodes[0];
            // Get the node's bounding rect center for hit testing
            let node_center = self.with_graph(|g| {
                g.world.get::<NodePosition>(node_id).map(|p| {
                    let is_reroute = g.world.get::<nodegraph_core::graph::reroute::IsReroute>(node_id).is_some();
                    if is_reroute {
                        // Reroute position IS the center
                        layout::Vec2::new(p.x, p.y)
                    } else {
                        // Regular node: position is top-left, center is offset
                        let num_ports = g.node_ports(node_id).len();
                        let h = layout::HEADER_HEIGHT + num_ports as f64 * layout::PORT_HEIGHT;
                        layout::Vec2::new(p.x + layout::NODE_MIN_WIDTH / 2.0, p.y + h / 2.0)
                    }
                })
            });
            if let Some(center) = node_center {
                // Use connection-only hit test at the node center
                let conn_hit = {
                    let editor = self.editor.borrow();
                    let graph = editor.current_graph();
                    let cache = layout::LayoutCache::compute(graph);
                    nodegraph_core::interaction::hit_test_connection(&cache, center)
                };
                if let Some(conn_id) = conn_hit {
                    // Don't insert onto a connection that already involves this node
                    let involves_self = self.with_graph(|g| {
                        g.world.get::<ConnectionEndpoints>(conn_id).map(|ep| {
                            let src_owner = g.world.get::<nodegraph_core::graph::port::PortOwner>(ep.source_port).map(|o| o.0);
                            let tgt_owner = g.world.get::<nodegraph_core::graph::port::PortOwner>(ep.target_port).map(|o| o.0);
                            src_owner == Some(node_id) || tgt_owner == Some(node_id)
                        }).unwrap_or(false)
                    });
                    if !involves_self {
                        self.insert_node_on_connection(node_id, conn_id);
                    }
                }
            }
            self.drop_target_connection.set(None);
        }

        let mut connections_may_have_changed = false;
        for effect in &effects {
            match effect {
                SideEffect::NodesMoved => {
                    self.sync_all_positions();
                    // Update drop_target_connection during single-node drag
                    if is_now_dragging && dragged_nodes.len() == 1 {
                        let node_id = dragged_nodes[0];
                        let center = self.with_graph(|g| {
                            g.world.get::<NodePosition>(node_id).map(|p| {
                                let is_reroute = g.world.get::<nodegraph_core::graph::reroute::IsReroute>(node_id).is_some();
                                if is_reroute {
                                    layout::Vec2::new(p.x, p.y)
                                } else {
                                    let np = g.node_ports(node_id).len();
                                    let h = layout::HEADER_HEIGHT + np as f64 * layout::PORT_HEIGHT;
                                    layout::Vec2::new(p.x + layout::NODE_MIN_WIDTH / 2.0, p.y + h / 2.0)
                                }
                            })
                        });
                        if let Some(c) = center {
                            let conn = {
                                let editor = self.editor.borrow();
                                let graph = editor.current_graph();
                                let cache = layout::LayoutCache::compute(graph);
                                nodegraph_core::interaction::hit_test_connection(&cache, c)
                            };
                            // Filter out self-connections
                            let conn = conn.filter(|&cid| {
                                !self.with_graph(|g| {
                                    g.world.get::<ConnectionEndpoints>(cid).map(|ep| {
                                        let so = g.world.get::<nodegraph_core::graph::port::PortOwner>(ep.source_port).map(|o| o.0);
                                        let to = g.world.get::<nodegraph_core::graph::port::PortOwner>(ep.target_port).map(|o| o.0);
                                        so == Some(node_id) || to == Some(node_id)
                                    }).unwrap_or(false)
                                })
                            });
                            self.drop_target_connection.set(conn);
                        }
                    }
                }
                SideEffect::SelectionChanged => self.sync_selection(),
                SideEffect::ConnectionCreated(conn_id) => {
                    self.connection_list.lock_mut().push_cloned(*conn_id);
                    connections_may_have_changed = true;
                }
                SideEffect::PreviewWire { .. } => {}
                SideEffect::BoxSelectRect { rect } => {
                    self.box_select_rect.set(Some((rect.x, rect.y, rect.w, rect.h)));
                }
                SideEffect::ClearTransient => {
                    self.preview_wire.set(None);
                    self.box_select_rect.set(None);
                    self.connecting_from.set(None);
                    self.drop_target_port.set(None);
                    connections_may_have_changed = true;
                }
                SideEffect::ConnectionFailed => { self.connecting_from.set(None); }
                SideEffect::FrameSelected(frame_id) => {
                    self.selected_frames.set(vec![*frame_id]);
                }
                SideEffect::FrameDeselected => {
                    if !self.selected_frames.get_cloned().is_empty() {
                        self.selected_frames.set(Vec::new());
                    }
                }
            }
        }

        { let ctrl = self.controller.borrow(); self.pan.set(ctrl.viewport.pan); self.zoom.set(ctrl.viewport.zoom); }
        {
            let ctrl = self.controller.borrow();
            match &ctrl.state {
                InteractionState::CuttingLinks { points } => {
                    self.cut_line_points.set(points.iter().map(|p| (p.x, p.y)).collect());
                }
                _ => { if !self.cut_line_points.get_cloned().is_empty() { self.cut_line_points.set(Vec::new()); } }
            }
        }
        if connections_may_have_changed { self.reconcile_connections(); }
    }

    fn reconcile_connections(&self) {
        let editor = self.editor.borrow();
        let graph = editor.current_graph();
        let live: std::collections::HashSet<EntityId> = graph.world.query::<ConnectionEndpoints>()
            .map(|(id, _)| id).collect();
        drop(editor);
        let mut list = self.connection_list.lock_mut();
        let mut i = 0;
        while i < list.len() {
            if !live.contains(&list[i]) {
                let dead_id = list[i];
                list.remove(i);
                if let Some(cb) = self.on_disconnect.borrow().as_ref() { cb(dead_id); }
            } else {
                i += 1;
            }
        }
    }

    pub fn reconcile_connections_pub(&self) { self.reconcile_connections(); }
    pub fn full_sync_pub(&self) { self.full_sync(); }

    fn insert_node_on_connection(&self, node_id: EntityId, conn_id: EntityId) {
        use nodegraph_core::graph::port::PortSocketType;

        let (endpoints, src_type, tgt_type, node_ports) = self.with_graph(|g| {
            let ep = g.world.get::<ConnectionEndpoints>(conn_id).cloned();
            let st = ep.as_ref().and_then(|e| g.world.get::<PortSocketType>(e.source_port).map(|s| s.0));
            let tt = ep.as_ref().and_then(|e| g.world.get::<PortSocketType>(e.target_port).map(|s| s.0));
            let ports = g.node_ports(node_id).to_vec();
            (ep, st, tt, ports)
        });

        let Some(ep) = endpoints else { return };
        let Some(src_type) = src_type else { return };
        let Some(tgt_type) = tgt_type else { return };

        // Find first compatible input port (src_type → port)
        let compatible_in = self.with_graph(|g| {
            node_ports.iter().find(|&&pid| {
                let dir = g.world.get::<PortDirection>(pid).copied();
                let pt = g.world.get::<PortSocketType>(pid).map(|s| s.0);
                dir == Some(PortDirection::Input) && pt.map(|t| src_type.is_compatible_with(&t)).unwrap_or(false)
            }).copied()
        });

        // Find first compatible output port (port → tgt_type)
        let compatible_out = self.with_graph(|g| {
            node_ports.iter().find(|&&pid| {
                let dir = g.world.get::<PortDirection>(pid).copied();
                let pt = g.world.get::<PortSocketType>(pid).map(|s| s.0);
                dir == Some(PortDirection::Output) && pt.map(|t| t.is_compatible_with(&tgt_type)).unwrap_or(false)
            }).copied()
        });

        if let (Some(n_in), Some(n_out)) = (compatible_in, compatible_out) {
            self.with_graph_mut(|g| g.disconnect(conn_id));
            let _ = self.connect_ports_no_undo(ep.source_port, n_in);
            let _ = self.connect_ports_no_undo(n_out, ep.target_port);
            self.reconcile_connections();
        }
    }

    // ============================================================
    // Group operations (all undoable via snapshot)
    // ============================================================

    pub fn group_selected(self: &Rc<Self>) {
        let selected = self.selection.get_cloned();
        if selected.is_empty() { return; }
        self.save_undo();
        self.editor.borrow_mut().group_nodes(&selected);
        self.full_sync();
    }

    pub fn ungroup_selected(self: &Rc<Self>) {
        let selected = self.selection.get_cloned();
        self.save_undo();
        for &nid in &selected { self.editor.borrow_mut().ungroup(nid); }
        self.full_sync();
    }

    pub fn enter_group(&self, group_node_id: EntityId) {
        let mut editor = self.editor.borrow_mut();
        if editor.enter_group(group_node_id) {
            let id = editor.current_graph_id();
            let label = editor.graph_label(id);
            drop(editor);
            self.current_graph_id.set(id);
            self.breadcrumb.lock_mut().push_cloned((id, label));
            self.full_sync();
        }
    }

    pub fn navigate_to_graph(&self, graph_id: EntityId) {
        let mut editor = self.editor.borrow_mut();
        if editor.navigate_to(graph_id) {
            drop(editor);
            self.current_graph_id.set(graph_id);
            let editor = self.editor.borrow();
            let bc: Vec<(EntityId, String)> = editor.breadcrumb().iter()
                .map(|&id| (id, editor.graph_label(id))).collect();
            drop(editor);
            let mut lock = self.breadcrumb.lock_mut();
            lock.clear();
            for item in bc { lock.push_cloned(item); }
            self.full_sync();
        }
    }

    pub fn exit_group(&self) {
        let mut editor = self.editor.borrow_mut();
        if editor.exit_group() {
            let id = editor.current_graph_id();
            drop(editor);
            self.navigate_to_graph(id);
        }
    }

    pub fn create_frame_around_selected(self: &Rc<Self>) {
        // Don't create frame if a frame is already selected (prevents framing frames)
        if !self.selected_frames.get_cloned().is_empty() { return; }
        let selected = self.selection.get_cloned();
        if selected.is_empty() { return; }
        self.save_undo();
        self.with_graph_mut(|g| { g.add_frame("Frame", [80, 80, 120], &selected); });
        self.full_sync();
    }

    pub fn add_group_io_at(self: &Rc<Self>, kind: GroupIOKind, position: (f64, f64)) {
        self.save_undo();
        self.create_group_io_node(kind, position);
        self.full_sync();
    }

    fn create_group_io_node(&self, kind: GroupIOKind, position: (f64, f64)) {
        let io_node = self.editor.borrow_mut().add_group_io_node(kind, SocketType::Any, "");
        if let Some(node_id) = io_node {
            self.with_graph_mut(|g| {
                if let Some(pos) = g.world.get_mut::<nodegraph_core::graph::node::NodePosition>(node_id) {
                    pos.x = position.0;
                    pos.y = position.1;
                }
            });
        }
    }

    // ============================================================
    // Keyboard commands (all undoable via snapshot)
    // ============================================================

    pub fn delete_selected(self: &Rc<Self>) {
        let selected_nodes = self.selection.get_cloned();
        let selected_frames = self.selected_frames.get_cloned();
        if selected_nodes.is_empty() && selected_frames.is_empty() { return; }
        self.save_undo();
        for &nid in &selected_nodes {
            // Auto-reconnect: bridge upstream→downstream before deleting
            let bridges = self.find_bridge_connections(nid);
            self.with_graph_mut(|g| g.remove_node(nid));
            for (upstream, downstream) in bridges {
                let _ = self.connect_ports_no_undo(upstream, downstream);
            }
        }
        for &fid in &selected_frames { self.with_graph_mut(|g| g.remove_frame(fid)); }
        self.selected_frames.set(Vec::new());
        self.full_sync();
    }

    /// Find connection pairs that flow through a node and could be bridged on deletion.
    fn find_bridge_connections(&self, node_id: EntityId) -> Vec<(EntityId, EntityId)> {
        use nodegraph_core::graph::port::PortSocketType;

        self.with_graph(|g| {
            let ports = g.node_ports(node_id).to_vec();

            // Collect all incoming connections (upstream_source_port → this node's input)
            let mut upstreams: Vec<EntityId> = Vec::new();
            for &pid in &ports {
                if g.world.get::<PortDirection>(pid).copied() != Some(PortDirection::Input) { continue; }
                for &conn_id in g.port_connections(pid) {
                    if let Some(ep) = g.world.get::<ConnectionEndpoints>(conn_id) {
                        if ep.target_port == pid {
                            upstreams.push(ep.source_port);
                        }
                    }
                }
            }

            // Collect all outgoing connections (this node's output → downstream_target_port)
            let mut downstreams: Vec<EntityId> = Vec::new();
            for &pid in &ports {
                if g.world.get::<PortDirection>(pid).copied() != Some(PortDirection::Output) { continue; }
                for &conn_id in g.port_connections(pid) {
                    if let Some(ep) = g.world.get::<ConnectionEndpoints>(conn_id) {
                        if ep.source_port == pid {
                            downstreams.push(ep.target_port);
                        }
                    }
                }
            }

            // Match upstream→downstream by type compatibility
            let mut bridges = Vec::new();
            let mut used_downstreams = std::collections::HashSet::new();
            for &upstream in &upstreams {
                let up_type = g.world.get::<PortSocketType>(upstream).map(|s| s.0);
                for &downstream in &downstreams {
                    if used_downstreams.contains(&downstream) { continue; }
                    let down_type = g.world.get::<PortSocketType>(downstream).map(|s| s.0);
                    if let (Some(ut), Some(dt)) = (up_type, down_type) {
                        if ut.is_compatible_with(&dt) {
                            bridges.push((upstream, downstream));
                            used_downstreams.insert(downstream);
                            break;
                        }
                    }
                }
            }
            bridges
        })
    }

    pub fn undo(self: &Rc<Self>) {
        { let mut editor = self.editor.borrow_mut(); self.history.borrow_mut().undo(&mut *editor); }
        self.full_sync();
    }

    pub fn redo(self: &Rc<Self>) {
        { let mut editor = self.editor.borrow_mut(); self.history.borrow_mut().redo(&mut *editor); }
        self.full_sync();
    }

    pub fn duplicate_selected(self: &Rc<Self>) {
        let selected = self.selection.get_cloned();
        if selected.is_empty() { return; }
        self.save_undo();
        // Simple duplicate: copy + paste with offset
        let clipboard = nodegraph_core::commands::copy_nodes(
            self.editor.borrow().current_graph(), &selected);
        nodegraph_core::commands::paste_nodes(
            self.editor.borrow_mut().current_graph_mut(), &clipboard, (30.0, 30.0));
        self.full_sync();
    }

    pub fn toggle_mute_selected(self: &Rc<Self>) {
        self.save_undo();
        for &nid in &self.selection.get_cloned() {
            let muted = self.with_graph(|g| g.world.get::<MuteState>(nid).map(|m| m.0).unwrap_or(false));
            self.with_graph_mut(|g| g.world.insert(nid, MuteState(!muted)));
        }
        self.sync_all_headers();
    }

    pub fn toggle_collapse_selected(self: &Rc<Self>) {
        self.save_undo();
        for &nid in &self.selection.get_cloned() {
            let c = self.with_graph(|g| g.world.get::<NodeHeader>(nid).map(|h| h.collapsed).unwrap_or(false));
            self.with_graph_mut(|g| { if let Some(h) = g.world.get_mut::<NodeHeader>(nid) { h.collapsed = !c; } });
        }
        self.sync_all_headers();
    }

    pub fn select_all(self: &Rc<Self>) {
        let current = self.selection.get_cloned();
        let all: Vec<EntityId> = self.with_graph(|g| g.world.query::<NodeHeader>().map(|(id, _)| id).collect());
        if current.len() == all.len() { self.controller.borrow_mut().selection.clear(); }
        else { self.controller.borrow_mut().selection.set(all); }
        self.sync_selection();
    }

    // ============================================================
    // Sync
    // ============================================================

    /// Delta-sync a MutableVec with a new set of live IDs.
    /// Only removes dead entries and adds new ones — avoids clearing the entire list.
    fn sync_entity_list(list: &MutableVec<EntityId>, live: &[EntityId]) {
        let live_set: std::collections::HashSet<EntityId> = live.iter().copied().collect();
        let current: Vec<EntityId> = list.lock_ref().iter().copied().collect();
        let cur_set: std::collections::HashSet<EntityId> = current.iter().copied().collect();

        if live_set == cur_set { return; }

        let mut lock = list.lock_mut();
        // Remove dead entries
        let mut i = 0;
        while i < lock.len() {
            if !live_set.contains(&lock[i]) { lock.remove(i); } else { i += 1; }
        }
        // Add new entries
        for &id in live {
            if !cur_set.contains(&id) { lock.push_cloned(id); }
        }
    }

    fn full_sync(&self) {
        let current_gid = self.editor.borrow().current_graph_id();
        let graph_changed = self.last_synced_graph.get() != Some(current_gid);
        self.last_synced_graph.set(Some(current_gid));

        let nodes: Vec<EntityId> = self.with_graph(|g| g.world.query::<NodeHeader>().map(|(id, _)| id).collect());
        if graph_changed {
            // Entity IDs are per-world — stale IDs from previous graph must not survive
            { let mut l = self.node_list.lock_mut(); l.clear(); for &id in &nodes { l.push_cloned(id); } }
            self.node_positions.borrow_mut().clear();
            self.node_headers.borrow_mut().clear();
            self.frame_bounds.borrow_mut().clear();
        } else {
            Self::sync_entity_list(&self.node_list, &nodes);
        }
        {
            let editor = self.editor.borrow();
            let graph = editor.current_graph();
            let mut positions = self.node_positions.borrow_mut();
            let mut headers = self.node_headers.borrow_mut();
            if !graph_changed {
                let node_set: std::collections::HashSet<EntityId> = nodes.iter().copied().collect();
                positions.retain(|id, _| node_set.contains(id));
                headers.retain(|id, _| node_set.contains(id));
            }
            for &nid in &nodes {
                let pos = graph.world.get::<NodePosition>(nid).map(|p| (p.x, p.y)).unwrap_or((0.0, 0.0));
                if let Some(m) = positions.get(&nid) {
                    if m.get() != pos { m.set(pos); }
                } else { positions.insert(nid, Mutable::new(pos)); }
                let h = graph.world.get::<NodeHeader>(nid).cloned()
                    .unwrap_or(NodeHeader { title: "?".into(), color: [100,100,100], collapsed: false });
                if let Some(m) = headers.get(&nid) {
                    let cur = m.get_cloned();
                    if cur.title != h.title || cur.color != h.color || cur.collapsed != h.collapsed { m.set(h); }
                } else { headers.insert(nid, Mutable::new(h)); }
            }
        }
        let conns: Vec<EntityId> = self.with_graph(|g| g.world.query::<ConnectionEndpoints>().map(|(id, _)| id).collect());
        if graph_changed {
            { let mut l = self.connection_list.lock_mut(); l.clear(); for &id in &conns { l.push_cloned(id); } }
        } else {
            Self::sync_entity_list(&self.connection_list, &conns);
        }
        {
            let editor = self.editor.borrow();
            let graph = editor.current_graph();
            let frames: Vec<EntityId> = graph.world.query::<nodegraph_core::graph::frame::FrameRect>().map(|(id, _)| id).collect();
            let mut bounds = self.frame_bounds.borrow_mut();
            let frame_set: std::collections::HashSet<EntityId> = frames.iter().copied().collect();
            bounds.retain(|id, _| frame_set.contains(id));
            for &fid in &frames {
                if let Some(members) = graph.world.get::<nodegraph_core::graph::frame::FrameMembers>(fid) {
                    let rect = layout::compute_frame_rect(graph, &members.0);
                    let val = (rect.x, rect.y, rect.w, rect.h);
                    if let Some(m) = bounds.get(&fid) {
                        if m.get() != val { m.set(val); }
                    } else { bounds.insert(fid, Mutable::new(val)); }
                }
            }
            drop(editor);
            // Always clear+rebuild frames (few elements, ensures color/label changes render)
            { let mut l = self.frame_list.lock_mut(); l.clear(); for &id in &frames { l.push_cloned(id); } }
        }
        self.sync_selection();
        self.recompute_graph_bounds();
    }

    fn sync_all_positions(&self) {
        let mut moved = Vec::new();
        let editor = self.editor.borrow();
        let graph = editor.current_graph();
        let positions = self.node_positions.borrow();
        for (id, mutable) in positions.iter() {
            if let Some(pos) = graph.world.get::<NodePosition>(*id) {
                let new_pos = (pos.x, pos.y);
                if mutable.get() != new_pos {
                    mutable.set(new_pos);
                    moved.push((*id, pos.x, pos.y));
                }
            }
        }
        // Recompute frame bounds from member positions
        let bounds = self.frame_bounds.borrow();
        for (&fid, mutable) in bounds.iter() {
            if let Some(members) = graph.world.get::<nodegraph_core::graph::frame::FrameMembers>(fid) {
                let rect = layout::compute_frame_rect(graph, &members.0);
                mutable.set((rect.x, rect.y, rect.w, rect.h));
            }
        }
        drop(editor);
        self.recompute_graph_bounds();
        if !moved.is_empty() {
            if let Some(cb) = self.on_node_moved.borrow().as_ref() { cb(&moved); }
        }
    }

    fn recompute_graph_bounds(&self) {
        let editor = self.editor.borrow();
        let graph = editor.current_graph();
        let mut min_x = f64::MAX;
        let mut min_y = f64::MAX;
        let mut max_x = f64::MIN;
        let mut max_y = f64::MIN;
        for (_, pos) in graph.world.query::<NodePosition>() {
            min_x = min_x.min(pos.x);
            min_y = min_y.min(pos.y);
            let num_ports = 3; // rough estimate
            let h = layout::HEADER_HEIGHT + num_ports as f64 * layout::PORT_HEIGHT;
            max_x = max_x.max(pos.x + layout::NODE_MIN_WIDTH);
            max_y = max_y.max(pos.y + h);
        }
        if min_x < f64::MAX {
            self.graph_bounds.set((min_x, min_y, max_x, max_y));
        }
    }

    fn sync_all_headers(&self) {
        let editor = self.editor.borrow();
        let graph = editor.current_graph();
        let headers = self.node_headers.borrow();
        for (id, m) in headers.iter() { if let Some(h) = graph.world.get::<NodeHeader>(*id) { m.set(h.clone()); } }
    }

    fn sync_selection(&self) {
        let sel = self.controller.borrow().selection.selected.clone();
        self.selection.set(sel.clone());
        if let Some(cb) = self.on_selection_changed.borrow().as_ref() { cb(&sel); }
    }

    pub fn get_node_position_signal(&self, node_id: EntityId) -> Option<Mutable<(f64, f64)>> {
        self.node_positions.borrow().get(&node_id).cloned()
    }
    pub fn get_node_header_signal(&self, node_id: EntityId) -> Option<Mutable<NodeHeader>> {
        self.node_headers.borrow().get(&node_id).cloned()
    }
    pub fn get_frame_bounds_signal(&self, frame_id: EntityId) -> Option<Mutable<(f64, f64, f64, f64)>> {
        self.frame_bounds.borrow().get(&frame_id).cloned()
    }
}
