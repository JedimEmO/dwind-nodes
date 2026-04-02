use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use wasm_bindgen::JsCast;
use futures_signals::signal::Mutable;
use futures_signals::signal_vec::MutableVec;

use nodegraph_core::commands::{
    CommandHistory, MoveNodesCommand,
    RemoveNodeCommand, DuplicateNodesCommand, MuteNodeCommand, CollapseNodeCommand,
};
use nodegraph_core::graph::{GraphEditor, NodeGraph};
use nodegraph_core::graph::node::{NodeHeader, NodePosition, MuteState};
use nodegraph_core::graph::port::PortDirection;
use nodegraph_core::graph::connection::ConnectionEndpoints;
use nodegraph_core::interaction::{InteractionController, InputEvent, SideEffect, InteractionState};
use nodegraph_core::layout::{self, BezierPath, Vec2, PORT_RADIUS};
use nodegraph_core::search::NodeTypeRegistry;
use nodegraph_core::store::EntityId;
use nodegraph_core::types::socket_type::SocketType;

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
    pub history: Rc<RefCell<CommandHistory>>,
    pub controller: Rc<RefCell<InteractionController>>,

    pub node_list: MutableVec<EntityId>,
    pub connection_list: MutableVec<EntityId>,

    pub node_positions: Rc<RefCell<HashMap<EntityId, Mutable<(f64, f64)>>>>,
    pub node_headers: Rc<RefCell<HashMap<EntityId, Mutable<NodeHeader>>>>,

    pub selection: Mutable<Vec<EntityId>>,
    pub pan: Mutable<(f64, f64)>,
    pub zoom: Mutable<f64>,

    pub connecting_from: Mutable<Option<(EntityId, SocketType, bool)>>,
    pub drop_target_port: Mutable<Option<EntityId>>,
    pub preview_wire: Mutable<Option<BezierPath>>,
    pub box_select_rect: Mutable<Option<(f64, f64, f64, f64)>>,
    pub cut_line_points: Mutable<Vec<(f64, f64)>>,

    pub registry: Rc<RefCell<NodeTypeRegistry>>,
    pub search_menu: Mutable<Option<(f64, f64)>>,
    pub pending_connection: Mutable<Option<(EntityId, SocketType, bool)>>,

    /// Current graph ID — drives which graph's nodes/connections are rendered
    pub current_graph_id: Mutable<EntityId>,
    /// Breadcrumb path for navigation
    pub breadcrumb: MutableVec<(EntityId, String)>,
}

impl GraphSignals {
    pub fn new() -> Rc<Self> {
        let editor = GraphEditor::new();
        let root_id = editor.root_graph_id();
        Rc::new(Self {
            editor: Rc::new(RefCell::new(editor)),
            history: Rc::new(RefCell::new(CommandHistory::new())),
            controller: Rc::new(RefCell::new(InteractionController::new())),
            node_list: MutableVec::new(),
            connection_list: MutableVec::new(),
            node_positions: Rc::new(RefCell::new(HashMap::new())),
            node_headers: Rc::new(RefCell::new(HashMap::new())),
            selection: Mutable::new(Vec::new()),
            pan: Mutable::new((0.0, 0.0)),
            zoom: Mutable::new(1.0),
            connecting_from: Mutable::new(None),
            drop_target_port: Mutable::new(None),
            preview_wire: Mutable::new(None),
            box_select_rect: Mutable::new(None),
            cut_line_points: Mutable::new(Vec::new()),
            registry: Rc::new(RefCell::new(NodeTypeRegistry::new())),
            search_menu: Mutable::new(None),
            pending_connection: Mutable::new(None),
            current_graph_id: Mutable::new(root_id),
            breadcrumb: MutableVec::new_with_values(vec![(root_id, "Root".to_string())]),
        })
    }

    /// Backwards-compatible accessor: borrows editor and returns the Ref.
    /// Callers can then call `.current_graph()` on the returned guard.
    /// This allows `gs.graph().current_graph().world.get::<...>()`
    pub fn graph(&self) -> std::cell::Ref<'_, GraphEditor> {
        self.editor.borrow()
    }

    pub fn graph_mut(&self) -> std::cell::RefMut<'_, GraphEditor> {
        self.editor.borrow_mut()
    }

    pub fn with_graph<R>(&self, f: impl FnOnce(&NodeGraph) -> R) -> R {
        let editor = self.editor.borrow();
        f(editor.current_graph())
    }

    /// Borrow the current graph mutably (convenience).
    pub fn with_graph_mut<R>(&self, f: impl FnOnce(&mut NodeGraph) -> R) -> R {
        let mut editor = self.editor.borrow_mut();
        f(editor.current_graph_mut())
    }

    pub fn add_node(&self, title: &str, position: (f64, f64), ports: Vec<(PortDirection, SocketType, String)>) -> EntityId {
        let node_id = self.with_graph_mut(|graph| {
            let nid = graph.add_node(title, position);
            for (dir, st, label) in &ports {
                graph.add_port(nid, *dir, *st, label);
            }
            nid
        });
        let header = self.with_graph(|graph| {
            graph.world.get::<NodeHeader>(node_id).cloned()
                .unwrap_or(NodeHeader { title: title.to_string(), color: [100, 100, 100], collapsed: false })
        });
        self.node_positions.borrow_mut().insert(node_id, Mutable::new(position));
        self.node_headers.borrow_mut().insert(node_id, Mutable::new(header));
        self.node_list.lock_mut().push_cloned(node_id);
        node_id
    }

    pub fn spawn_from_registry(&self, type_id: &str, position: (f64, f64)) {
        let def = match self.registry.borrow().get(type_id) {
            Some(d) => d.clone(),
            None => return,
        };
        let mut all_ports: Vec<(PortDirection, SocketType, String)> = Vec::new();
        for p in &def.input_ports { all_ports.push((p.direction, p.socket_type, p.label.clone())); }
        for p in &def.output_ports { all_ports.push((p.direction, p.socket_type, p.label.clone())); }

        // Use AddNodeCommand for undo support
        use nodegraph_core::commands::AddNodeCommand;
        let cmd = AddNodeCommand::new(&def.display_name, position, all_ports.clone());
        {
            let mut editor = self.editor.borrow_mut();
            let mut history = self.history.borrow_mut();
            history.execute(Box::new(cmd), editor.current_graph_mut());
        }
        // Find the newly added node
        let node_id = self.with_graph(|g| {
            g.world.query::<NodeHeader>()
                .filter(|(_, h)| h.title == def.display_name)
                .map(|(id, _)| id)
                .last()
        });
        let node_id = match node_id {
            Some(id) => id,
            None => { self.pending_connection.set(None); self.search_menu.set(None); return; }
        };
        // Sync signals for the new node
        let header = self.with_graph(|g| g.world.get::<NodeHeader>(node_id).cloned()
            .unwrap_or(NodeHeader { title: def.display_name.clone(), color: [100,100,100], collapsed: false }));
        self.node_positions.borrow_mut().insert(node_id, Mutable::new(position));
        self.node_headers.borrow_mut().insert(node_id, Mutable::new(header));
        self.node_list.lock_mut().push_cloned(node_id);

        if let Some((src_port, src_type, from_output)) = self.pending_connection.get() {
            let new_ports = self.with_graph(|g| g.node_ports(node_id).to_vec());
            for &pid in &new_ports {
                let info = self.with_graph(|g| {
                    let dir = g.world.get::<PortDirection>(pid).copied();
                    let st = g.world.get::<nodegraph_core::graph::port::PortSocketType>(pid).map(|s| s.0);
                    (dir, st)
                });
                if let (Some(dir), Some(st)) = info {
                    if is_valid_connection_target(from_output, src_type, dir, st) {
                        self.connect_ports(src_port, pid);
                        break;
                    }
                }
            }
        }
        self.pending_connection.set(None);
        self.search_menu.set(None);
    }

    pub fn open_search_menu(&self, world_x: f64, world_y: f64) {
        self.pending_connection.set(None);
        self.search_menu.set(Some((world_x, world_y)));
        wasm_bindgen_futures::spawn_local(async {
            let promise = js_sys::Promise::resolve(&wasm_bindgen::JsValue::NULL);
            let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
            if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
                if let Some(el) = doc.query_selector("[data-search-menu] input").ok().flatten() {
                    if let Ok(html_el) = el.dyn_into::<web_sys::HtmlElement>() {
                        let _ = html_el.focus();
                    }
                }
            }
        });
    }

    pub fn close_search_menu(&self) {
        self.search_menu.set(None);
        self.pending_connection.set(None);
    }

    pub fn connect_ports(&self, source: EntityId, target: EntityId) -> Option<EntityId> {
        use nodegraph_core::commands::ConnectCommand;
        let conns_before: std::collections::HashSet<EntityId> = self.with_graph(|g| {
            g.world.query::<ConnectionEndpoints>().map(|(id, _)| id).collect()
        });
        {
            let mut editor = self.editor.borrow_mut();
            let mut history = self.history.borrow_mut();
            history.execute(Box::new(ConnectCommand::new(source, target)), editor.current_graph_mut());
        }
        // Find the new connection by diffing
        let conn_id = self.with_graph(|g| {
            g.world.query::<ConnectionEndpoints>()
                .map(|(id, _)| id)
                .find(|id| !conns_before.contains(id))
        })?;
        self.connection_list.lock_mut().push_cloned(conn_id);
        self.reconcile_connections(); // remove any replaced connection
        self.adapt_io_ports_after_connect(source, target);
        Some(conn_id)
    }

    /// After a connection, adapt Group IO port types to match what they're connected to.
    fn adapt_io_ports_after_connect(&self, port_a: EntityId, port_b: EntityId) {
        let (a_type, b_type, a_is_io, b_is_io) = self.with_graph(|g| {
            let at = g.world.get::<nodegraph_core::graph::port::PortSocketType>(port_a).map(|s| s.0);
            let bt = g.world.get::<nodegraph_core::graph::port::PortSocketType>(port_b).map(|s| s.0);
            let a_io = g.world.get::<nodegraph_core::graph::port::PortOwner>(port_a)
                .and_then(|o| g.world.get::<nodegraph_core::graph::GroupIOKind>(o.0)).is_some();
            let b_io = g.world.get::<nodegraph_core::graph::port::PortOwner>(port_b)
                .and_then(|o| g.world.get::<nodegraph_core::graph::GroupIOKind>(o.0)).is_some();
            (at, bt, a_io, b_io)
        });
        let mut editor = self.editor.borrow_mut();
        if a_is_io { if let Some(bt) = b_type { editor.adapt_group_io_port(port_a, bt); } }
        if b_is_io { if let Some(at) = a_type { editor.adapt_group_io_port(port_b, at); } }
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

    pub fn start_connecting(self: &Rc<Self>, port_id: EntityId, _screen: Vec2, world: Vec2) {
        let (from_output, socket_type) = self.with_graph(|g| {
            let from_output = g.world.get::<PortDirection>(port_id).map(|d| *d == PortDirection::Output).unwrap_or(false);
            let st = g.world.get::<nodegraph_core::graph::port::PortSocketType>(port_id).map(|s| s.0).unwrap_or(SocketType::Float);
            (from_output, st)
        });
        self.connecting_from.set(Some((port_id, socket_type, from_output)));
        self.controller.borrow_mut().state = InteractionState::ConnectingPort {
            source_port: port_id, from_output, cursor_world: world,
        };
    }

    pub fn handle_input(self: &Rc<Self>, event: InputEvent) {
        let is_connecting = matches!(self.controller.borrow().state, InteractionState::ConnectingPort { .. });

        if is_connecting {
            if let InputEvent::MouseUp { .. } = &event {
                let source_port = match &self.controller.borrow().state {
                    InteractionState::ConnectingPort { source_port, .. } => *source_port,
                    _ => unreachable!(),
                };
                if let Some(target_port) = self.drop_target_port.get() {
                    let result = self.with_graph_mut(|g| g.connect(source_port, target_port));
                    if let Ok(conn_id) = result {
                        self.connection_list.lock_mut().push_cloned(conn_id);
                        self.reconcile_connections();
                        self.adapt_io_ports_after_connect(source_port, target_port);
                    }
                    self.connecting_from.set(None);
                } else {
                    let world = match &event {
                        InputEvent::MouseUp { world, .. } => *world,
                        _ => Vec2::new(0.0, 0.0),
                    };
                    if let Some(cf) = self.connecting_from.get() {
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

        let pre_drag_positions: Option<Vec<(EntityId, f64, f64)>> = {
            let ctrl = self.controller.borrow();
            match &ctrl.state {
                InteractionState::DraggingNodes { node_ids, .. } => {
                    Some(self.with_graph(|g| {
                        node_ids.iter().filter_map(|&id| {
                            g.world.get::<NodePosition>(id).map(|p| (id, p.x, p.y))
                        }).collect()
                    }))
                }
                _ => None,
            }
        };
        let was_dragging = matches!(self.controller.borrow().state, InteractionState::DraggingNodes { .. });

        let effects = {
            let mut editor = self.editor.borrow_mut();
            let mut ctrl = self.controller.borrow_mut();
            ctrl.handle_event(event, editor.current_graph_mut())
        };

        let is_now_idle = matches!(self.controller.borrow().state, InteractionState::Idle);

        if was_dragging && is_now_idle {
            if let Some(pre_positions) = pre_drag_positions {
                let done = self.with_graph(|g| {
                    for &(id, pre_x, pre_y) in &pre_positions {
                        if let Some(pos) = g.world.get::<NodePosition>(id) {
                            let dx = pos.x - pre_x;
                            let dy = pos.y - pre_y;
                            if dx.abs() > 0.1 || dy.abs() > 0.1 {
                                return Some((dx, dy));
                            }
                        }
                    }
                    None
                });
                if let Some((dx, dy)) = done {
                    let node_ids = pre_positions.iter().map(|&(id, _, _)| id).collect();
                    self.history.borrow_mut().push_already_executed(Box::new(MoveNodesCommand {
                        node_ids, delta_x: dx, delta_y: dy,
                    }));
                }
            }
        }

        let mut connections_may_have_changed = false;
        for effect in &effects {
            match effect {
                SideEffect::NodesMoved => self.sync_all_positions(),
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
        while i < list.len() { if !live.contains(&list[i]) { list.remove(i); } else { i += 1; } }
    }

    // ============================================================
    // Group operations
    // ============================================================

    pub fn group_selected(self: &Rc<Self>) {
        let selected = self.selection.get_cloned();
        if selected.is_empty() { return; }
        let result = self.editor.borrow_mut().group_nodes(&selected);
        if result.is_some() {
            self.full_sync();
        }
    }

    pub fn ungroup_selected(self: &Rc<Self>) {
        let selected = self.selection.get_cloned();
        for &nid in &selected {
            self.editor.borrow_mut().ungroup(nid);
        }
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
            // Rebuild breadcrumb from editor state
            let editor = self.editor.borrow();
            let bc: Vec<(EntityId, String)> = editor.breadcrumb().iter()
                .map(|&id| (id, editor.graph_label(id)))
                .collect();
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

    /// Select a single node (syncs both controller and signal).
    pub fn select_single(&self, node_id: EntityId) {
        self.controller.borrow_mut().selection.clear();
        self.controller.borrow_mut().selection.select(node_id);
        self.selection.set(vec![node_id]);
    }

    /// Add a port to a selected Group IO node (+ key).
    pub fn add_group_io_port(self: &Rc<Self>) {
        let selected = self.selection.get_cloned();
        if selected.len() != 1 { return; }
        let nid = selected[0];

        // Check if it's a Group IO node
        let is_io = self.with_graph(|g| g.world.get::<nodegraph_core::graph::GroupIOKind>(nid).is_some());
        if !is_io { return; }

        self.editor.borrow_mut().add_group_io_port(nid, SocketType::Any, "");
        self.full_sync();
    }

    // ============================================================
    // Keyboard commands
    // ============================================================

    pub fn delete_selected(self: &Rc<Self>) {
        let selected = self.selection.get_cloned();
        if selected.is_empty() { return; }
        for &nid in &selected {
            let mut editor = self.editor.borrow_mut();
            let mut h = self.history.borrow_mut();
            h.execute(Box::new(RemoveNodeCommand::new(nid)), editor.current_graph_mut());
        }
        self.full_sync();
    }

    pub fn undo(self: &Rc<Self>) {
        { let mut editor = self.editor.borrow_mut(); self.history.borrow_mut().undo(editor.current_graph_mut()); }
        self.full_sync();
    }

    pub fn redo(self: &Rc<Self>) {
        { let mut editor = self.editor.borrow_mut(); self.history.borrow_mut().redo(editor.current_graph_mut()); }
        self.full_sync();
    }

    pub fn duplicate_selected(self: &Rc<Self>) {
        let selected = self.selection.get_cloned();
        if selected.is_empty() { return; }
        { let mut editor = self.editor.borrow_mut();
          self.history.borrow_mut().execute(Box::new(DuplicateNodesCommand::new(selected, (30.0, 30.0))), editor.current_graph_mut()); }
        self.full_sync();
    }

    pub fn toggle_mute_selected(self: &Rc<Self>) {
        for &nid in &self.selection.get_cloned() {
            let muted = self.with_graph(|g| g.world.get::<MuteState>(nid).map(|m| m.0).unwrap_or(false));
            let mut editor = self.editor.borrow_mut();
            self.history.borrow_mut().execute(Box::new(MuteNodeCommand { node_id: nid, muted: !muted }), editor.current_graph_mut());
        }
        self.sync_all_headers();
    }

    pub fn toggle_collapse_selected(self: &Rc<Self>) {
        for &nid in &self.selection.get_cloned() {
            let c = self.with_graph(|g| g.world.get::<NodeHeader>(nid).map(|h| h.collapsed).unwrap_or(false));
            let mut editor = self.editor.borrow_mut();
            self.history.borrow_mut().execute(Box::new(CollapseNodeCommand { node_id: nid, collapsed: !c }), editor.current_graph_mut());
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

    fn full_sync(&self) {
        let nodes: Vec<EntityId> = self.with_graph(|g| g.world.query::<NodeHeader>().map(|(id, _)| id).collect());

        { let mut l = self.node_list.lock_mut(); l.clear(); for &id in &nodes { l.push_cloned(id); } }

        {
            let editor = self.editor.borrow();
            let graph = editor.current_graph();
            let mut positions = self.node_positions.borrow_mut();
            let mut headers = self.node_headers.borrow_mut();
            for &nid in &nodes {
                let pos = graph.world.get::<NodePosition>(nid).map(|p| (p.x, p.y)).unwrap_or((0.0, 0.0));
                if let Some(m) = positions.get(&nid) { m.set(pos); } else { positions.insert(nid, Mutable::new(pos)); }
                let h = graph.world.get::<NodeHeader>(nid).cloned()
                    .unwrap_or(NodeHeader { title: "?".into(), color: [100,100,100], collapsed: false });
                if let Some(m) = headers.get(&nid) { m.set(h); } else { headers.insert(nid, Mutable::new(h)); }
            }
        }

        { let conns: Vec<EntityId> = self.with_graph(|g| g.world.query::<ConnectionEndpoints>().map(|(id, _)| id).collect());
          let mut l = self.connection_list.lock_mut(); l.clear(); for &id in &conns { l.push_cloned(id); } }

        self.sync_selection();
    }

    fn sync_all_positions(&self) {
        let editor = self.editor.borrow();
        let graph = editor.current_graph();
        let positions = self.node_positions.borrow();
        for (id, mutable) in positions.iter() {
            if let Some(pos) = graph.world.get::<NodePosition>(*id) { mutable.set((pos.x, pos.y)); }
        }
    }

    fn sync_all_headers(&self) {
        let editor = self.editor.borrow();
        let graph = editor.current_graph();
        let headers = self.node_headers.borrow();
        for (id, m) in headers.iter() { if let Some(h) = graph.world.get::<NodeHeader>(*id) { m.set(h.clone()); } }
    }

    fn sync_selection(&self) {
        self.selection.set(self.controller.borrow().selection.selected.clone());
    }

    /// Convenience: borrow the current graph's node_count.
    pub fn node_count(&self) -> usize {
        self.with_graph(|g| g.node_count())
    }

    /// Convenience: borrow the current graph's connection_count.
    pub fn connection_count(&self) -> usize {
        self.with_graph(|g| g.connection_count())
    }

    pub fn get_node_position_signal(&self, node_id: EntityId) -> Option<Mutable<(f64, f64)>> {
        self.node_positions.borrow().get(&node_id).cloned()
    }
    pub fn get_node_header_signal(&self, node_id: EntityId) -> Option<Mutable<NodeHeader>> {
        self.node_headers.borrow().get(&node_id).cloned()
    }
}
