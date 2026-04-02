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
use nodegraph_core::graph::NodeGraph;
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
    pub graph: Rc<RefCell<NodeGraph>>,
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

    /// Node type registry — populated by the app
    pub registry: Rc<RefCell<NodeTypeRegistry>>,
    /// Search menu state: Some((world_x, world_y)) = open at position, None = closed
    pub search_menu: Mutable<Option<(f64, f64)>>,
    /// Pending connection from a noodle drop — (port_id, socket_type, from_output)
    /// When set, search menu filters to compatible types and auto-connects after spawn.
    pub pending_connection: Mutable<Option<(EntityId, SocketType, bool)>>,
}

impl GraphSignals {
    pub fn new() -> Rc<Self> {
        Rc::new(Self {
            graph: Rc::new(RefCell::new(NodeGraph::new())),
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
        })
    }

    pub fn add_node(&self, title: &str, position: (f64, f64), ports: Vec<(PortDirection, SocketType, String)>) -> EntityId {
        let node_id = {
            let mut graph = self.graph.borrow_mut();
            let nid = graph.add_node(title, position);
            for (dir, st, label) in &ports {
                graph.add_port(nid, *dir, *st, label);
            }
            nid
        };
        let header = self.graph.borrow().world.get::<NodeHeader>(node_id).cloned()
            .unwrap_or(NodeHeader { title: title.to_string(), color: [100, 100, 100], collapsed: false });
        self.node_positions.borrow_mut().insert(node_id, Mutable::new(position));
        self.node_headers.borrow_mut().insert(node_id, Mutable::new(header));
        self.node_list.lock_mut().push_cloned(node_id);
        node_id
    }

    /// Spawn a node from a registry type definition at a world position.
    /// If there's a pending connection, auto-connect to the first compatible port.
    pub fn spawn_from_registry(&self, type_id: &str, position: (f64, f64)) {
        let def = match self.registry.borrow().get(type_id) {
            Some(d) => d.clone(),
            None => return,
        };

        let mut all_ports: Vec<(PortDirection, SocketType, String)> = Vec::new();
        for p in &def.input_ports {
            all_ports.push((p.direction, p.socket_type, p.label.clone()));
        }
        for p in &def.output_ports {
            all_ports.push((p.direction, p.socket_type, p.label.clone()));
        }

        let node_id = self.add_node(&def.display_name, position, all_ports);

        // Auto-connect if there's a pending connection from noodle drop
        if let Some((src_port, src_type, from_output)) = self.pending_connection.get() {
            let graph = self.graph.borrow();
            let new_ports = graph.node_ports(node_id).to_vec();
            drop(graph);

            // Find first compatible port on the new node
            for &pid in &new_ports {
                let graph = self.graph.borrow();
                let dir = graph.world.get::<PortDirection>(pid).copied();
                let st = graph.world.get::<nodegraph_core::graph::port::PortSocketType>(pid).map(|s| s.0);
                drop(graph);

                if let (Some(dir), Some(st)) = (dir, st) {
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
        // Focus the search input after dominator flushes the display change
        wasm_bindgen_futures::spawn_local(async {
            // Yield to let dominator update display:none → display:block
            let promise = js_sys::Promise::resolve(&wasm_bindgen::JsValue::NULL);
            let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
            // Now focus the input
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
        let conn_id = self.graph.borrow_mut().connect(source, target).ok()?;
        self.connection_list.lock_mut().push_cloned(conn_id);
        Some(conn_id)
    }

    /// Port world position — pure function of graph state via layout module.
    /// Same computation used by hit testing and SVG rendering.
    pub fn port_world_pos(&self, port_id: EntityId) -> Option<Vec2> {
        let graph = self.graph.borrow();
        layout::compute_port_world_position(&graph, port_id)
    }

    fn find_port_at(&self, world: Vec2) -> Option<EntityId> {
        let graph = self.graph.borrow();
        let cache = layout::LayoutCache::compute(&graph);
        let hit_radius = PORT_RADIUS + 5.0;
        let mut best: Option<(EntityId, f64)> = None;

        for (_, node_layout) in &cache.layouts {
            for &(pid, pos) in node_layout.input_port_positions.iter()
                .chain(node_layout.output_port_positions.iter())
            {
                let dist = world.distance_to(pos);
                if dist <= hit_radius && best.map_or(true, |(_, d)| dist < d) {
                    best = Some((pid, dist));
                }
            }
        }
        best.map(|(id, _)| id)
    }

    pub fn start_connecting(self: &Rc<Self>, port_id: EntityId, _screen: Vec2, world: Vec2) {
        let graph = self.graph.borrow();
        let from_output = graph.world.get::<PortDirection>(port_id)
            .map(|d| *d == PortDirection::Output).unwrap_or(false);
        let socket_type = graph.world.get::<nodegraph_core::graph::port::PortSocketType>(port_id)
            .map(|s| s.0).unwrap_or(SocketType::Float);
        drop(graph);
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
                    let result = self.graph.borrow_mut().connect(source_port, target_port);
                    if let Ok(conn_id) = result {
                        self.connection_list.lock_mut().push_cloned(conn_id);
                        self.reconcile_connections();
                    }
                    self.connecting_from.set(None);
                } else {
                    // Dropped on empty canvas — open search menu filtered to compatible types
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
                        let graph = self.graph.borrow();
                        let tgt_dir = graph.world.get::<PortDirection>(pid).copied()?;
                        let tgt_type = graph.world.get::<nodegraph_core::graph::port::PortSocketType>(pid)?.0;
                        if is_valid_connection_target(from_output, src_type, tgt_dir, tgt_type) { Some(pid) } else { None }
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
                    let graph = self.graph.borrow();
                    Some(node_ids.iter().filter_map(|&id| {
                        graph.world.get::<NodePosition>(id).map(|p| (id, p.x, p.y))
                    }).collect())
                }
                _ => None,
            }
        };
        let was_dragging = matches!(self.controller.borrow().state, InteractionState::DraggingNodes { .. });

        let effects = {
            let mut graph = self.graph.borrow_mut();
            let mut ctrl = self.controller.borrow_mut();
            ctrl.handle_event(event, &mut graph)
        };

        let is_now_idle = matches!(self.controller.borrow().state, InteractionState::Idle);

        if was_dragging && is_now_idle {
            if let Some(pre_positions) = pre_drag_positions {
                let graph = self.graph.borrow();
                for &(id, pre_x, pre_y) in &pre_positions {
                    if let Some(pos) = graph.world.get::<NodePosition>(id) {
                        let dx = pos.x - pre_x;
                        let dy = pos.y - pre_y;
                        if dx.abs() > 0.1 || dy.abs() > 0.1 {
                            drop(graph);
                            let node_ids = pre_positions.iter().map(|&(id, _, _)| id).collect();
                            self.history.borrow_mut().push_already_executed(Box::new(MoveNodesCommand {
                                node_ids, delta_x: dx, delta_y: dy,
                            }));
                            break;
                        }
                    }
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
        let graph = self.graph.borrow();
        let live: std::collections::HashSet<EntityId> = graph.world.query::<ConnectionEndpoints>()
            .map(|(id, _)| id).collect();
        drop(graph);
        let mut list = self.connection_list.lock_mut();
        let mut i = 0;
        while i < list.len() { if !live.contains(&list[i]) { list.remove(i); } else { i += 1; } }
    }

    pub fn delete_selected(self: &Rc<Self>) {
        let selected = self.selection.get_cloned();
        if selected.is_empty() { return; }
        for &nid in &selected {
            let mut g = self.graph.borrow_mut(); let mut h = self.history.borrow_mut();
            h.execute(Box::new(RemoveNodeCommand::new(nid)), &mut g);
        }
        self.full_sync();
    }

    pub fn undo(self: &Rc<Self>) {
        { let mut g = self.graph.borrow_mut(); self.history.borrow_mut().undo(&mut g); }
        self.full_sync();
    }

    pub fn redo(self: &Rc<Self>) {
        { let mut g = self.graph.borrow_mut(); self.history.borrow_mut().redo(&mut g); }
        self.full_sync();
    }

    pub fn duplicate_selected(self: &Rc<Self>) {
        let selected = self.selection.get_cloned();
        if selected.is_empty() { return; }
        { let mut g = self.graph.borrow_mut();
          self.history.borrow_mut().execute(Box::new(DuplicateNodesCommand::new(selected, (30.0, 30.0))), &mut g); }
        self.full_sync();
    }

    pub fn toggle_mute_selected(self: &Rc<Self>) {
        for &nid in &self.selection.get_cloned() {
            let muted = self.graph.borrow().world.get::<MuteState>(nid).map(|m| m.0).unwrap_or(false);
            let mut g = self.graph.borrow_mut();
            self.history.borrow_mut().execute(Box::new(MuteNodeCommand { node_id: nid, muted: !muted }), &mut g);
        }
        self.sync_all_headers();
    }

    pub fn toggle_collapse_selected(self: &Rc<Self>) {
        for &nid in &self.selection.get_cloned() {
            let c = self.graph.borrow().world.get::<NodeHeader>(nid).map(|h| h.collapsed).unwrap_or(false);
            let mut g = self.graph.borrow_mut();
            self.history.borrow_mut().execute(Box::new(CollapseNodeCommand { node_id: nid, collapsed: !c }), &mut g);
        }
        self.sync_all_headers();
    }

    pub fn select_all(self: &Rc<Self>) {
        let current = self.selection.get_cloned();
        let all: Vec<EntityId> = self.graph.borrow().world.query::<NodeHeader>().map(|(id, _)| id).collect();
        if current.len() == all.len() { self.controller.borrow_mut().selection.clear(); }
        else { self.controller.borrow_mut().selection.set(all); }
        self.sync_selection();
    }

    fn full_sync(&self) {
        let nodes: Vec<EntityId> = self.graph.borrow().world.query::<NodeHeader>().map(|(id, _)| id).collect();

        { let mut l = self.node_list.lock_mut(); l.clear(); for &id in &nodes { l.push_cloned(id); } }

        { let graph = self.graph.borrow();
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

        { let conns: Vec<EntityId> = self.graph.borrow().world.query::<ConnectionEndpoints>().map(|(id, _)| id).collect();
          let mut l = self.connection_list.lock_mut(); l.clear(); for &id in &conns { l.push_cloned(id); } }

        self.sync_selection();
    }

    fn sync_all_positions(&self) {
        let graph = self.graph.borrow();
        let positions = self.node_positions.borrow();
        for (id, mutable) in positions.iter() {
            if let Some(pos) = graph.world.get::<NodePosition>(*id) { mutable.set((pos.x, pos.y)); }
        }
    }

    fn sync_all_headers(&self) {
        let graph = self.graph.borrow();
        let headers = self.node_headers.borrow();
        for (id, m) in headers.iter() { if let Some(h) = graph.world.get::<NodeHeader>(*id) { m.set(h.clone()); } }
    }

    fn sync_selection(&self) {
        self.selection.set(self.controller.borrow().selection.selected.clone());
    }

    pub fn get_node_position_signal(&self, node_id: EntityId) -> Option<Mutable<(f64, f64)>> {
        self.node_positions.borrow().get(&node_id).cloned()
    }
    pub fn get_node_header_signal(&self, node_id: EntityId) -> Option<Mutable<NodeHeader>> {
        self.node_headers.borrow().get(&node_id).cloned()
    }
}
