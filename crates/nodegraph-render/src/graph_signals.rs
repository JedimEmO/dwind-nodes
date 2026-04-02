use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use futures_signals::signal::Mutable;
use futures_signals::signal_vec::MutableVec;

use nodegraph_core::commands::CommandHistory;
use nodegraph_core::graph::NodeGraph;
use nodegraph_core::graph::node::{NodeHeader, NodePosition};
use nodegraph_core::graph::port::PortDirection;
use nodegraph_core::graph::connection::ConnectionEndpoints;
use nodegraph_core::interaction::{InteractionController, InputEvent, SideEffect};
use nodegraph_core::layout::{self, BezierPath, Vec2};
use nodegraph_core::store::EntityId;
use nodegraph_core::types::socket_type::SocketType;

pub struct GraphSignals {
    pub graph: Rc<RefCell<NodeGraph>>,
    pub history: Rc<RefCell<CommandHistory>>,
    pub controller: Rc<RefCell<InteractionController>>,

    pub node_list: MutableVec<EntityId>,
    pub connection_list: MutableVec<EntityId>,

    pub node_positions: Rc<RefCell<HashMap<EntityId, Mutable<(f64, f64)>>>>,
    pub node_headers: Rc<RefCell<HashMap<EntityId, Mutable<NodeHeader>>>>,
    pub connection_paths: Rc<RefCell<HashMap<EntityId, Mutable<String>>>>,

    /// Port offset relative to its parent node's top-left corner (in unscaled CSS pixels).
    /// Measured once from the DOM after port circles are inserted.
    /// port_world_pos = node_position + port_offset
    pub port_offsets: Rc<RefCell<HashMap<EntityId, (f64, f64)>>>,
    /// Which node owns each port (for looking up node position)
    pub port_owners: Rc<RefCell<HashMap<EntityId, EntityId>>>,

    pub selection: Mutable<Vec<EntityId>>,

    pub pan: Mutable<(f64, f64)>,
    pub zoom: Mutable<f64>,

    pub preview_wire: Mutable<Option<BezierPath>>,
    pub box_select_rect: Mutable<Option<(f64, f64, f64, f64)>>,
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
            connection_paths: Rc::new(RefCell::new(HashMap::new())),
            port_offsets: Rc::new(RefCell::new(HashMap::new())),
            port_owners: Rc::new(RefCell::new(HashMap::new())),
            selection: Mutable::new(Vec::new()),
            pan: Mutable::new((0.0, 0.0)),
            zoom: Mutable::new(1.0),
            preview_wire: Mutable::new(None),
            box_select_rect: Mutable::new(None),
        })
    }

    pub fn add_node(&self, title: &str, position: (f64, f64), ports: Vec<(PortDirection, SocketType, String)>) -> EntityId {
        let node_id = {
            let mut graph = self.graph.borrow_mut();
            let nid = graph.add_node(title, position);
            for (dir, st, label) in &ports {
                let pid = graph.add_port(nid, *dir, *st, label);
                self.port_owners.borrow_mut().insert(pid, nid);
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

    pub fn connect_ports(&self, source: EntityId, target: EntityId) -> Option<EntityId> {
        let conn_id = {
            let mut graph = self.graph.borrow_mut();
            graph.connect(source, target).ok()?
        };

        self.sync_connection(conn_id);
        self.connection_list.lock_mut().push_cloned(conn_id);
        Some(conn_id)
    }

    /// Called by port circle DOM elements after insertion.
    /// `offset` is the port circle center relative to the node div's top-left, in unscaled px.
    pub fn report_port_offset(&self, port_id: EntityId, offset_x: f64, offset_y: f64) {
        self.port_offsets.borrow_mut().insert(port_id, (offset_x, offset_y));
        self.sync_connections_for_port(port_id);
    }

    /// Get a port's world position from its owner node's position + measured offset.
    fn port_world_pos(&self, port_id: EntityId) -> Option<Vec2> {
        let offsets = self.port_offsets.borrow();
        let owners = self.port_owners.borrow();
        let positions = self.node_positions.borrow();

        let &(ox, oy) = offsets.get(&port_id)?;
        let &owner_id = owners.get(&port_id)?;
        let pos_mutable = positions.get(&owner_id)?;
        let (nx, ny) = pos_mutable.get();

        Some(Vec2::new(nx + ox, ny + oy))
    }

    pub fn handle_input(self: &Rc<Self>, event: InputEvent) {
        let effects = {
            let mut graph = self.graph.borrow_mut();
            let mut ctrl = self.controller.borrow_mut();
            ctrl.handle_event(event, &mut graph)
        };

        for effect in effects {
            match effect {
                SideEffect::NodesMoved => self.sync_all_positions(),
                SideEffect::SelectionChanged => self.sync_selection(),
                SideEffect::ConnectionCreated(conn_id) => {
                    self.sync_connection(conn_id);
                    self.connection_list.lock_mut().push_cloned(conn_id);
                }
                SideEffect::PreviewWire { path } => {
                    self.preview_wire.set(Some(path));
                }
                SideEffect::BoxSelectRect { rect } => {
                    self.box_select_rect.set(Some((rect.x, rect.y, rect.w, rect.h)));
                }
                SideEffect::ClearTransient => {
                    self.preview_wire.set(None);
                    self.box_select_rect.set(None);
                }
                SideEffect::ConnectionFailed => {}
            }
        }

        let ctrl = self.controller.borrow();
        self.pan.set(ctrl.viewport.pan);
        self.zoom.set(ctrl.viewport.zoom);
    }

    fn sync_all_positions(&self) {
        let graph = self.graph.borrow();
        let positions = self.node_positions.borrow();
        for (id, mutable) in positions.iter() {
            if let Some(pos) = graph.world.get::<NodePosition>(*id) {
                mutable.set((pos.x, pos.y));
            }
        }
        drop(positions);
        drop(graph);
        self.sync_all_connections();
    }

    fn sync_connection(&self, conn_id: EntityId) {
        let graph = self.graph.borrow();
        if let Some(ep) = graph.world.get::<ConnectionEndpoints>(conn_id) {
            let src = self.port_world_pos(ep.source_port)
                .or_else(|| layout::compute_port_world_position(&graph, ep.source_port));
            let tgt = self.port_world_pos(ep.target_port)
                .or_else(|| layout::compute_port_world_position(&graph, ep.target_port));

            if let (Some(src), Some(tgt)) = (src, tgt) {
                let path = layout::compute_connection_path(src, tgt);
                let d = path.to_svg_d();
                let mut paths = self.connection_paths.borrow_mut();
                if let Some(mutable) = paths.get(&conn_id) {
                    mutable.set(d);
                } else {
                    paths.insert(conn_id, Mutable::new(d));
                }
            }
        }
    }

    fn sync_connections_for_port(&self, port_id: EntityId) {
        let graph = self.graph.borrow();
        let conn_ids: Vec<EntityId> = graph.port_connections(port_id).to_vec();
        drop(graph);
        for conn_id in conn_ids {
            self.sync_connection(conn_id);
        }
    }

    fn sync_all_connections(&self) {
        let graph = self.graph.borrow();
        let conn_ids: Vec<EntityId> = graph.world.query::<ConnectionEndpoints>()
            .map(|(id, _)| id)
            .collect();
        drop(graph);
        for conn_id in conn_ids {
            self.sync_connection(conn_id);
        }
    }

    fn sync_selection(&self) {
        let ctrl = self.controller.borrow();
        self.selection.set(ctrl.selection.selected.clone());
    }

    pub fn get_node_position_signal(&self, node_id: EntityId) -> Option<Mutable<(f64, f64)>> {
        self.node_positions.borrow().get(&node_id).cloned()
    }

    pub fn get_node_header_signal(&self, node_id: EntityId) -> Option<Mutable<NodeHeader>> {
        self.node_headers.borrow().get(&node_id).cloned()
    }

    pub fn get_connection_path_signal(&self, conn_id: EntityId) -> Option<Mutable<String>> {
        self.connection_paths.borrow().get(&conn_id).cloned()
    }
}
