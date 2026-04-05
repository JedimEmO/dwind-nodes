pub mod node;
pub mod port;
pub mod connection;
pub mod group;
pub mod frame;
pub mod reroute;

use std::collections::HashMap;
use crate::store::{EntityId, World};
use crate::types::socket_type::SocketType;
use node::{NodeHeader, NodePosition, NodeTypeId};
use port::{PortDirection, PortOwner, PortSocketType, PortLabel, PortIndex};
use connection::ConnectionEndpoints;

#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionError {
    /// Source port entity does not exist or is not a port
    InvalidSourcePort,
    /// Target port entity does not exist or is not a port
    InvalidTargetPort,
    /// Both ports have the same direction (both inputs or both outputs)
    SameDirection,
    /// Socket types are not compatible
    IncompatibleTypes(SocketType, SocketType),
    /// Cannot connect a port to another port on the same node
    SameNode,
}

/// A single node graph containing nodes, ports, connections, and frames.
///
/// Use [`add_node`](Self::add_node) to create nodes, [`add_port`](Self::add_port) to add
/// typed ports, and [`connect`](Self::connect) to wire them together. Supports serialization
/// via [`serialize`](Self::serialize) / [`deserialize`](Self::deserialize).
///
/// For multi-graph hierarchies (groups/subgraphs), use [`GraphEditor`] instead.
#[derive(Clone)]
pub struct NodeGraph {
    pub world: World,
    ports_by_node: HashMap<EntityId, Vec<EntityId>>,
    connections_by_port: HashMap<EntityId, Vec<EntityId>>,
}

impl NodeGraph {
    pub fn new() -> Self {
        Self {
            world: World::new(),
            ports_by_node: HashMap::new(),
            connections_by_port: HashMap::new(),
        }
    }

    pub fn new_with_start(start_index: u32) -> Self {
        Self {
            world: World::new_with_start(start_index),
            ports_by_node: HashMap::new(),
            connections_by_port: HashMap::new(),
        }
    }

    pub fn add_node(&mut self, title: &str, position: (f64, f64)) -> EntityId {
        let id = self.world.spawn();
        self.world.insert(id, NodeHeader {
            title: title.to_string(),
            color: [100, 100, 100],
            collapsed: false,
        });
        self.world.insert(id, NodePosition {
            x: position.0,
            y: position.1,
        });
        self.ports_by_node.insert(id, Vec::new());
        id
    }

    pub fn add_port(
        &mut self,
        node: EntityId,
        direction: PortDirection,
        socket_type: SocketType,
        label: &str,
    ) -> EntityId {
        let port_id = self.world.spawn();
        self.world.insert(port_id, PortOwner(node));
        self.world.insert(port_id, direction);
        self.world.insert(port_id, PortSocketType(socket_type));
        self.world.insert(port_id, PortLabel(label.to_string()));

        // Compute port index within its direction on this node
        let ports = self.ports_by_node.entry(node).or_default();
        let index = ports
            .iter()
            .filter(|&&p| {
                self.world
                    .get::<PortDirection>(p)
                    .map(|d| *d == direction)
                    .unwrap_or(false)
            })
            .count() as u32;
        self.world.insert(port_id, PortIndex(index));

        ports.push(port_id);
        self.connections_by_port.insert(port_id, Vec::new());
        port_id
    }

    /// Validates and returns the normalized (output_port, input_port) pair.
    fn validate_and_normalize(
        &self,
        source_port: EntityId,
        target_port: EntityId,
    ) -> Result<(EntityId, EntityId), ConnectionError> {
        let src_dir = self
            .world
            .get::<PortDirection>(source_port)
            .ok_or(ConnectionError::InvalidSourcePort)?;
        let tgt_dir = self
            .world
            .get::<PortDirection>(target_port)
            .ok_or(ConnectionError::InvalidTargetPort)?;

        if src_dir == tgt_dir {
            return Err(ConnectionError::SameDirection);
        }

        let (output_port, input_port) = if *src_dir == PortDirection::Output {
            (source_port, target_port)
        } else {
            (target_port, source_port)
        };

        let src_owner = self
            .world
            .get::<PortOwner>(output_port)
            .ok_or(ConnectionError::InvalidSourcePort)?;
        let tgt_owner = self
            .world
            .get::<PortOwner>(input_port)
            .ok_or(ConnectionError::InvalidTargetPort)?;

        if src_owner.0 == tgt_owner.0 {
            return Err(ConnectionError::SameNode);
        }

        let src_type = self
            .world
            .get::<PortSocketType>(output_port)
            .ok_or(ConnectionError::InvalidSourcePort)?;
        let tgt_type = self
            .world
            .get::<PortSocketType>(input_port)
            .ok_or(ConnectionError::InvalidTargetPort)?;

        if !src_type.0.is_compatible_with(&tgt_type.0) {
            return Err(ConnectionError::IncompatibleTypes(src_type.0, tgt_type.0));
        }

        Ok((output_port, input_port))
    }

    pub fn validate_connection(
        &self,
        source_port: EntityId,
        target_port: EntityId,
    ) -> Result<(), ConnectionError> {
        self.validate_and_normalize(source_port, target_port).map(|_| ())
    }

    /// Connect two ports. If the target input port already has a connection, it is replaced.
    /// Returns the connection entity ID on success.
    pub fn connect(
        &mut self,
        source_port: EntityId,
        target_port: EntityId,
    ) -> Result<EntityId, ConnectionError> {
        let (output_port, input_port) = self.validate_and_normalize(source_port, target_port)?;

        // Remove existing connection on the input port (inputs allow only one connection)
        let existing: Vec<EntityId> = self
            .connections_by_port
            .get(&input_port)
            .cloned()
            .unwrap_or_default();
        for conn_id in existing {
            self.disconnect(conn_id);
        }

        let conn_id = self.world.spawn();
        self.world.insert(
            conn_id,
            ConnectionEndpoints {
                source_port: output_port,
                target_port: input_port,
            },
        );

        self.connections_by_port
            .entry(output_port)
            .or_default()
            .push(conn_id);
        self.connections_by_port
            .entry(input_port)
            .or_default()
            .push(conn_id);

        Ok(conn_id)
    }

    pub fn disconnect(&mut self, connection: EntityId) {
        if let Some(endpoints) = self.world.get::<ConnectionEndpoints>(connection).cloned() {
            if let Some(conns) = self.connections_by_port.get_mut(&endpoints.source_port) {
                conns.retain(|&c| c != connection);
            }
            if let Some(conns) = self.connections_by_port.get_mut(&endpoints.target_port) {
                conns.retain(|&c| c != connection);
            }
            self.world.despawn(connection);
        }
    }

    pub fn remove_port(&mut self, port: EntityId) {
        // Remove all connections on this port
        let conns: Vec<EntityId> = self.connections_by_port
            .get(&port).cloned().unwrap_or_default();
        for conn_id in conns {
            self.disconnect(conn_id);
        }
        self.connections_by_port.remove(&port);
        // Remove from parent node's port list
        if let Some(owner) = self.world.get::<PortOwner>(port).map(|o| o.0) {
            if let Some(ports) = self.ports_by_node.get_mut(&owner) {
                ports.retain(|&p| p != port);
            }
        }
        self.world.despawn(port);
    }

    pub fn remove_node(&mut self, node: EntityId) {
        // Remove all connections on this node's ports
        if let Some(ports) = self.ports_by_node.get(&node).cloned() {
            for port_id in &ports {
                let conns: Vec<EntityId> = self
                    .connections_by_port
                    .get(port_id)
                    .cloned()
                    .unwrap_or_default();
                for conn_id in conns {
                    self.disconnect(conn_id);
                }
                self.connections_by_port.remove(port_id);
                self.world.despawn(*port_id);
            }
        }
        self.ports_by_node.remove(&node);

        // Remove from any frame's member list
        let frame_ids: Vec<EntityId> = self.world.query::<frame::FrameMembers>()
            .map(|(id, _)| id).collect();
        for fid in frame_ids {
            if let Some(members) = self.world.get_mut::<frame::FrameMembers>(fid) {
                members.0.retain(|&id| id != node);
            }
        }

        self.world.despawn(node);
    }

    pub fn node_ports(&self, node: EntityId) -> &[EntityId] {
        self.ports_by_node
            .get(&node)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    pub fn port_connections(&self, port: EntityId) -> &[EntityId] {
        self.connections_by_port
            .get(&port)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    pub fn connection_count(&self) -> usize {
        self.world.query::<ConnectionEndpoints>().count()
    }

    pub fn node_count(&self) -> usize {
        self.world.query::<NodeHeader>().count()
    }

    /// Topological sort of all nodes. Returns nodes in dependency order
    /// (sources first, sinks last). Returns Err with cycle participants if cycles exist.
    pub fn topological_sort(&self) -> Result<Vec<EntityId>, Vec<EntityId>> {
        use std::collections::{HashMap, VecDeque};

        let all_nodes: Vec<EntityId> = self.world.query::<NodeHeader>().map(|(id, _)| id).collect();
        let mut in_degree: HashMap<EntityId, usize> = all_nodes.iter().map(|&id| (id, 0)).collect();
        let mut adjacency: HashMap<EntityId, Vec<EntityId>> = all_nodes.iter().map(|&id| (id, Vec::new())).collect();

        // Build dependency graph from connections
        for (_, ep) in self.world.query::<ConnectionEndpoints>() {
            let src_node = self.world.get::<PortOwner>(ep.source_port).map(|o| o.0);
            let tgt_node = self.world.get::<PortOwner>(ep.target_port).map(|o| o.0);
            if let (Some(src), Some(tgt)) = (src_node, tgt_node) {
                if src != tgt {
                    adjacency.entry(src).or_default().push(tgt);
                    *in_degree.entry(tgt).or_default() += 1;
                }
            }
        }

        // Kahn's algorithm
        let mut queue: VecDeque<EntityId> = in_degree.iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(&id, _)| id)
            .collect();
        let mut sorted = Vec::with_capacity(all_nodes.len());

        while let Some(node) = queue.pop_front() {
            sorted.push(node);
            if let Some(neighbors) = adjacency.get(&node) {
                for &neighbor in neighbors {
                    if let Some(deg) = in_degree.get_mut(&neighbor) {
                        *deg -= 1;
                        if *deg == 0 { queue.push_back(neighbor); }
                    }
                }
            }
        }

        if sorted.len() == all_nodes.len() {
            Ok(sorted)
        } else {
            let cycle_nodes: Vec<EntityId> = in_degree.iter()
                .filter(|(_, &deg)| deg > 0)
                .map(|(&id, _)| id)
                .collect();
            Err(cycle_nodes)
        }
    }

    /// Convenience: nodes in evaluation order. Returns empty if cycles exist.
    pub fn eval_order(&self) -> Vec<EntityId> {
        self.topological_sort().unwrap_or_default()
    }

    /// Create a frame around the given nodes. Computes bounding rect with padding.
    pub fn add_frame(&mut self, label: &str, color: [u8; 3], member_ids: &[EntityId]) -> EntityId {
        use frame::{FrameLabel, FrameColor, FrameRect, FrameMembers};

        let rect = crate::layout::compute_frame_rect(self, member_ids);
        let frame_id = self.world.spawn();
        self.world.insert(frame_id, FrameLabel(label.to_string()));
        self.world.insert(frame_id, FrameColor(color));
        self.world.insert(frame_id, FrameRect { x: rect.x, y: rect.y, w: rect.w, h: rect.h });
        self.world.insert(frame_id, FrameMembers(member_ids.to_vec()));
        frame_id
    }

    /// Remove a frame without affecting its member nodes.
    pub fn remove_frame(&mut self, frame_id: EntityId) {
        self.world.despawn(frame_id);
    }

    pub fn frame_count(&self) -> usize {
        self.world.query::<frame::FrameRect>().count()
    }
}

impl Default for NodeGraph {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================
// GraphEditor — manages multiple NodeGraphs for group navigation
// ============================================================

use group::SubgraphRoot;

/// Marker component for Group Input/Output special nodes inside subgraphs.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum GroupIOKind {
    Input,
    Output,
}

/// Multi-graph editor managing a hierarchy of node graphs (root + subgraphs).
///
/// Provides node grouping via [`group_nodes`](Self::group_nodes), subgraph navigation
/// via [`enter_group`](Self::enter_group) / [`exit_group`](Self::exit_group), and full
/// serialization via [`serialize_editor`](Self::serialize_editor).
///
/// For rendering, wrap in [`GraphSignals`](crate::graph_signals::GraphSignals) and pass
/// to [`render_graph_editor`](crate::viewport_view::render_graph_editor).
#[derive(Clone)]
pub struct GraphEditor {
    graphs: HashMap<EntityId, NodeGraph>,
    root_graph_id: EntityId,
    current_graph_id: EntityId,
    breadcrumb: Vec<EntityId>,
    next_graph_id: u32,
    /// Counter for assigning unique entity ID ranges to subgraphs.
    /// Each subgraph gets entity IDs starting from `next_entity_start`.
    next_entity_start: u32,
    /// subgraph_id → (parent_graph_id, group_node_id)
    pub subgraph_parents: HashMap<EntityId, (EntityId, EntityId)>,
    /// Bidirectional mapping: subgraph IO port ↔ parent group node port.
    /// Key: (subgraph_id, IO port in subgraph), Value: corresponding port on group node in parent.
    pub io_port_mapping: HashMap<(EntityId, EntityId), EntityId>,
}

impl GraphEditor {
    pub fn new() -> Self {
        let root = NodeGraph::new();
        // Use a synthetic EntityId for the root graph
        let root_id = EntityId { index: 0, generation: crate::store::Generation::default() };
        let mut graphs = HashMap::new();
        graphs.insert(root_id, root);

        Self {
            graphs,
            root_graph_id: root_id,
            current_graph_id: root_id,
            breadcrumb: vec![root_id],
            next_graph_id: 1,
            next_entity_start: 10000,
            subgraph_parents: HashMap::new(),
            io_port_mapping: HashMap::new(),
        }
    }

    fn alloc_graph_id(&mut self) -> EntityId {
        let id = EntityId { index: self.next_graph_id, generation: crate::store::Generation::default() };
        self.next_graph_id += 1;
        id
    }

    pub fn root_graph_id(&self) -> EntityId { self.root_graph_id }
    pub fn current_graph_id(&self) -> EntityId { self.current_graph_id }
    pub fn breadcrumb(&self) -> &[EntityId] { &self.breadcrumb }

    pub fn current_graph(&self) -> &NodeGraph {
        self.graphs.get(&self.current_graph_id).expect("current graph must exist")
    }

    pub fn current_graph_mut(&mut self) -> &mut NodeGraph {
        self.graphs.get_mut(&self.current_graph_id).expect("current graph must exist")
    }

    pub fn graph(&self, id: EntityId) -> Option<&NodeGraph> {
        self.graphs.get(&id)
    }

    pub fn graph_mut(&mut self, id: EntityId) -> Option<&mut NodeGraph> {
        self.graphs.get_mut(&id)
    }

    /// Navigate into a group node's subgraph.
    pub fn enter_group(&mut self, group_node_id: EntityId) -> bool {
        let current = self.graphs.get(&self.current_graph_id).expect("current graph");
        if let Some(sub) = current.world.get::<SubgraphRoot>(group_node_id) {
            let subgraph_id = sub.0;
            if self.graphs.contains_key(&subgraph_id) {
                self.current_graph_id = subgraph_id;
                self.breadcrumb.push(subgraph_id);
                return true;
            }
        }
        false
    }

    /// Navigate back to a specific graph in the breadcrumb.
    pub fn navigate_to(&mut self, graph_id: EntityId) -> bool {
        if let Some(pos) = self.breadcrumb.iter().position(|&id| id == graph_id) {
            self.breadcrumb.truncate(pos + 1);
            self.current_graph_id = graph_id;
            true
        } else {
            false
        }
    }

    /// Navigate up one level.
    pub fn exit_group(&mut self) -> bool {
        if self.breadcrumb.len() > 1 {
            self.breadcrumb.pop();
            self.current_graph_id = *self.breadcrumb.last().unwrap();
            true
        } else {
            false
        }
    }

    /// Group selected nodes from the current graph into a new subgraph.
    /// Returns (group_node_id, subgraph_id) on success.
    pub fn group_nodes(&mut self, node_ids: &[EntityId]) -> Option<(EntityId, EntityId)> {
        if node_ids.is_empty() { return None; }

        let subgraph_id = self.alloc_graph_id();
        let start = self.next_entity_start;
        self.next_entity_start += 10000;
        let mut subgraph = NodeGraph::new_with_start(start);

        let current_id = self.current_graph_id;
        let parent = self.graphs.get_mut(&current_id)?;

        // Collect node data from parent
        let node_set: std::collections::HashSet<EntityId> = node_ids.iter().copied().collect();

        // Identify cut connections: connections where one end is in the selection and the other isn't
        let mut external_inputs: Vec<(EntityId, EntityId, SocketType, String)> = Vec::new(); // (ext_src_port, int_tgt_port, type, label)
        let mut external_outputs: Vec<(EntityId, EntityId, SocketType, String)> = Vec::new(); // (int_src_port, ext_tgt_port, type, label)
        let mut internal_connections: std::collections::HashSet<(EntityId, EntityId)> = std::collections::HashSet::new();

        for &nid in node_ids {
            for &pid in parent.node_ports(nid) {
                for &conn_id in parent.port_connections(pid) {
                    if let Some(ep) = parent.world.get::<ConnectionEndpoints>(conn_id) {
                        let src_owner = parent.world.get::<PortOwner>(ep.source_port).map(|o| o.0);
                        let tgt_owner = parent.world.get::<PortOwner>(ep.target_port).map(|o| o.0);

                        let src_in = src_owner.map(|o| node_set.contains(&o)).unwrap_or(false);
                        let tgt_in = tgt_owner.map(|o| node_set.contains(&o)).unwrap_or(false);

                        if src_in && tgt_in {
                            internal_connections.insert((ep.source_port, ep.target_port));
                        } else if src_in && !tgt_in {
                            // Output goes external
                            let st = parent.world.get::<PortSocketType>(ep.source_port).map(|s| s.0).unwrap_or(SocketType::Float);
                            let label = parent.world.get::<PortLabel>(ep.source_port).map(|l| l.0.clone()).unwrap_or_default();
                            external_outputs.push((ep.source_port, ep.target_port, st, label));
                        } else if !src_in && tgt_in {
                            // Input comes from external
                            let st = parent.world.get::<PortSocketType>(ep.target_port).map(|s| s.0).unwrap_or(SocketType::Float);
                            let label = parent.world.get::<PortLabel>(ep.target_port).map(|l| l.0.clone()).unwrap_or_default();
                            external_inputs.push((ep.source_port, ep.target_port, st, label));
                        }
                    }
                }
            }
        }

        // Create group node in parent with ports matching cut connections
        let group_node = parent.add_node("Group", (0.0, 0.0));
        // Position: average of selected nodes
        let mut avg_x = 0.0;
        let mut avg_y = 0.0;
        let mut count = 0.0;
        for &nid in node_ids {
            if let Some(pos) = parent.world.get::<NodePosition>(nid) {
                avg_x += pos.x;
                avg_y += pos.y;
                count += 1.0;
            }
        }
        if count > 0.0 {
            if let Some(pos) = parent.world.get_mut::<NodePosition>(group_node) {
                pos.x = avg_x / count;
                pos.y = avg_y / count;
            }
        }

        // Mark as group node with subgraph reference
        parent.world.insert(group_node, SubgraphRoot(subgraph_id));

        // Create group input/output ports on the group node
        let mut group_input_ports = Vec::new();
        for (_, _, st, label) in &external_inputs {
            let gp = parent.add_port(group_node, PortDirection::Input, *st, label);
            group_input_ports.push(gp);
        }
        let mut group_output_ports = Vec::new();
        for (_, _, st, label) in &external_outputs {
            let gp = parent.add_port(group_node, PortDirection::Output, *st, label);
            group_output_ports.push(gp);
        }

        // Reconnect external connections to group node ports
        for (i, (ext_src, _, _, _)) in external_inputs.iter().enumerate() {
            let _ = parent.connect(*ext_src, group_input_ports[i]);
        }
        for (i, (_, ext_tgt, _, _)) in external_outputs.iter().enumerate() {
            let _ = parent.connect(group_output_ports[i], *ext_tgt);
        }

        // Compute bounding box of nodes being moved into subgraph
        let mut min_x = f64::MAX;
        let mut min_y = f64::MAX;
        let mut max_x = f64::MIN;
        for &nid in node_ids {
            if let Some(pos) = parent.world.get::<NodePosition>(nid) {
                min_x = min_x.min(pos.x);
                min_y = min_y.min(pos.y);
                max_x = max_x.max(pos.x + crate::layout::NODE_MIN_WIDTH);
            }
        }
        if min_x == f64::MAX { min_x = 0.0; min_y = 0.0; max_x = 200.0; }
        let io_input_x = min_x - 200.0;
        let io_output_x = max_x + 80.0;

        // Create individual Group IO nodes inside subgraph — one per external connection
        let mut sub_input_ports = Vec::new();
        for (i, (_, _, st, label)) in external_inputs.iter().enumerate() {
            let io_node = subgraph.add_node(&format!("In: {}", label), (io_input_x, min_y + i as f64 * 50.0));
            subgraph.world.insert(io_node, GroupIOKind::Input);
            subgraph.world.insert(io_node, group::GroupIOLabel(label.clone()));
            let sp = subgraph.add_port(io_node, PortDirection::Output, *st, label);
            sub_input_ports.push(sp);
        }
        let mut sub_output_ports = Vec::new();
        for (i, (_, _, st, label)) in external_outputs.iter().enumerate() {
            let io_node = subgraph.add_node(&format!("Out: {}", label), (io_output_x, min_y + i as f64 * 50.0));
            subgraph.world.insert(io_node, GroupIOKind::Output);
            subgraph.world.insert(io_node, group::GroupIOLabel(label.clone()));
            let sp = subgraph.add_port(io_node, PortDirection::Input, *st, label);
            sub_output_ports.push(sp);
        }

        // Snapshot selected nodes from parent and recreate them in subgraph
        let mut old_to_new_port: HashMap<EntityId, EntityId> = HashMap::new();

        for &nid in node_ids {
            let header = parent.world.get::<NodeHeader>(nid).cloned()
                .unwrap_or(NodeHeader { title: "?".into(), color: [100,100,100], collapsed: false });
            let pos = parent.world.get::<NodePosition>(nid).cloned()
                .unwrap_or(NodePosition { x: 0.0, y: 0.0 });

            let new_nid = subgraph.add_node(&header.title, (pos.x, pos.y));
            if let Some(h) = subgraph.world.get_mut::<NodeHeader>(new_nid) {
                h.color = header.color;
                h.collapsed = header.collapsed;
            }

            // Preserve NodeTypeId and CustomBodyHeight
            if let Some(tid) = parent.world.get::<NodeTypeId>(nid).cloned() {
                subgraph.world.insert(new_nid, tid);
            }
            if let Some(cbh) = parent.world.get::<node::CustomBodyHeight>(nid).cloned() {
                subgraph.world.insert(new_nid, cbh);
            }

            // Recreate ports
            for &old_pid in parent.node_ports(nid) {
                let dir = parent.world.get::<PortDirection>(old_pid).copied()
                    .unwrap_or(PortDirection::Input);
                let st = parent.world.get::<PortSocketType>(old_pid).map(|s| s.0)
                    .unwrap_or(SocketType::Float);
                let label = parent.world.get::<PortLabel>(old_pid).map(|l| l.0.clone())
                    .unwrap_or_default();
                let new_pid = subgraph.add_port(new_nid, dir, st, &label);
                old_to_new_port.insert(old_pid, new_pid);
            }
        }

        // Recreate internal connections in subgraph
        for (old_src, old_tgt) in &internal_connections {
            if let (Some(&new_src), Some(&new_tgt)) = (old_to_new_port.get(old_src), old_to_new_port.get(old_tgt)) {
                let _ = subgraph.connect(new_src, new_tgt);
            }
        }

        // Connect IO nodes to the recreated internal ports
        for (i, (_, int_tgt_port, _, _)) in external_inputs.iter().enumerate() {
            if let Some(&new_tgt) = old_to_new_port.get(int_tgt_port) {
                let _ = subgraph.connect(sub_input_ports[i], new_tgt);
            }
        }
        for (i, (int_src_port, _, _, _)) in external_outputs.iter().enumerate() {
            if let Some(&new_src) = old_to_new_port.get(int_src_port) {
                let _ = subgraph.connect(new_src, sub_output_ports[i]);
            }
        }

        // Remove original nodes from parent
        for &nid in node_ids {
            parent.remove_node(nid);
        }

        self.graphs.insert(subgraph_id, subgraph);

        // Cache parent/child mappings
        self.subgraph_parents.insert(subgraph_id, (current_id, group_node));
        for (i, &sub_port) in sub_input_ports.iter().enumerate() {
            self.io_port_mapping.insert((subgraph_id, sub_port), group_input_ports[i]);
        }
        for (i, &sub_port) in sub_output_ports.iter().enumerate() {
            self.io_port_mapping.insert((subgraph_id, sub_port), group_output_ports[i]);
        }

        Some((group_node, subgraph_id))
    }

    /// Ungroup: dissolve a group node, moving subgraph nodes back into the parent.
    pub fn ungroup(&mut self, group_node_id: EntityId) -> bool {
        let current_id = self.current_graph_id;
        let subgraph_id = {
            let parent = self.graphs.get(&current_id).expect("current graph");
            match parent.world.get::<SubgraphRoot>(group_node_id) {
                Some(s) => s.0,
                None => return false,
            }
        };

        // Snapshot the group node's external connections before removing it
        let mut external_inputs: Vec<(EntityId, EntityId)> = Vec::new(); // (ext_src, group_input_port)
        let mut external_outputs: Vec<(EntityId, EntityId)> = Vec::new(); // (group_output_port, ext_tgt)
        {
            let parent = self.graphs.get(&current_id).expect("current graph");
            for &pid in parent.node_ports(group_node_id) {
                let dir = parent.world.get::<PortDirection>(pid).copied();
                for &conn_id in parent.port_connections(pid) {
                    if let Some(ep) = parent.world.get::<ConnectionEndpoints>(conn_id) {
                        match dir {
                            Some(PortDirection::Input) => external_inputs.push((ep.source_port, pid)),
                            Some(PortDirection::Output) => external_outputs.push((pid, ep.target_port)),
                            None => {}
                        }
                    }
                }
            }
        }

        // Snapshot subgraph nodes (excluding IO nodes) and their connections
        let subgraph = match self.graphs.get(&subgraph_id) { Some(g) => g, None => return false };

        let mut nodes_to_move: Vec<(NodeHeader, NodePosition, Vec<(PortDirection, SocketType, String, EntityId)>)> = Vec::new();
        let mut sub_connections: Vec<(EntityId, EntityId)> = Vec::new();

        for (nid, header) in subgraph.world.query::<NodeHeader>() {
            // Skip IO nodes
            if subgraph.world.get::<GroupIOKind>(nid).is_some() { continue; }
            let pos = subgraph.world.get::<NodePosition>(nid).cloned().unwrap_or(NodePosition { x: 0.0, y: 0.0 });
            let mut ports = Vec::new();
            for &pid in subgraph.node_ports(nid) {
                let dir = subgraph.world.get::<PortDirection>(pid).copied().unwrap_or(PortDirection::Input);
                let st = subgraph.world.get::<PortSocketType>(pid).map(|s| s.0).unwrap_or(SocketType::Float);
                let label = subgraph.world.get::<PortLabel>(pid).map(|l| l.0.clone()).unwrap_or_default();
                ports.push((dir, st, label, pid));
            }
            nodes_to_move.push((header.clone(), pos, ports));
        }

        // Collect internal connections (between non-IO nodes)
        for (_conn_id, ep) in subgraph.world.query::<ConnectionEndpoints>() {
            sub_connections.push((ep.source_port, ep.target_port));
        }

        // Find IO port → internal port connections (for reconnecting externals)
        let mut io_to_internal: HashMap<EntityId, EntityId> = HashMap::new(); // io_port → internal_port
        for (_conn_id, ep) in subgraph.world.query::<ConnectionEndpoints>() {
            let src_owner = subgraph.world.get::<PortOwner>(ep.source_port).map(|o| o.0);
            let tgt_owner = subgraph.world.get::<PortOwner>(ep.target_port).map(|o| o.0);
            let src_is_io = src_owner.and_then(|o| subgraph.world.get::<GroupIOKind>(o)).is_some();
            let tgt_is_io = tgt_owner.and_then(|o| subgraph.world.get::<GroupIOKind>(o)).is_some();
            if src_is_io && !tgt_is_io {
                io_to_internal.insert(ep.source_port, ep.target_port);
            }
            if tgt_is_io && !src_is_io {
                io_to_internal.insert(ep.target_port, ep.source_port);
            }
        }

        // Remove group node from parent (severs external connections)
        let parent = self.graphs.get_mut(&current_id).expect("current graph");
        parent.remove_node(group_node_id);

        // Recreate nodes in parent
        let mut old_to_new: HashMap<EntityId, EntityId> = HashMap::new();
        for (header, pos, ports) in &nodes_to_move {
            let new_nid = parent.add_node(&header.title, (pos.x, pos.y));
            if let Some(h) = parent.world.get_mut::<NodeHeader>(new_nid) {
                h.color = header.color;
                h.collapsed = header.collapsed;
            }
            for (dir, st, label, old_pid) in ports {
                let new_pid = parent.add_port(new_nid, *dir, *st, label);
                old_to_new.insert(*old_pid, new_pid);
            }
        }

        // Recreate internal connections
        for (old_src, old_tgt) in &sub_connections {
            if let (Some(&new_src), Some(&new_tgt)) = (old_to_new.get(old_src), old_to_new.get(old_tgt)) {
                let _ = parent.connect(new_src, new_tgt);
            }
        }

        // Reconnect externals: IO port → mapped to group port → mapped to internal port → new port
        for (ext_src, group_in_port) in &external_inputs {
            // Find the IO port that corresponds to this group port
            if let Some(io_port) = self.io_port_mapping.iter()
                .find(|(_, &gp)| gp == *group_in_port)
                .map(|((_, ip), _)| *ip)
            {
                if let Some(&internal_old) = io_to_internal.get(&io_port) {
                    if let Some(&new_tgt) = old_to_new.get(&internal_old) {
                        let _ = parent.connect(*ext_src, new_tgt);
                    }
                }
            }
        }
        for (group_out_port, ext_tgt) in &external_outputs {
            if let Some(io_port) = self.io_port_mapping.iter()
                .find(|(_, &gp)| gp == *group_out_port)
                .map(|((_, ip), _)| *ip)
            {
                if let Some(&internal_old) = io_to_internal.get(&io_port) {
                    if let Some(&new_src) = old_to_new.get(&internal_old) {
                        let _ = parent.connect(new_src, *ext_tgt);
                    }
                }
            }
        }

        // Clean up caches
        self.subgraph_parents.remove(&subgraph_id);
        self.io_port_mapping.retain(|(sid, _), _| *sid != subgraph_id);

        // Remove the subgraph
        self.graphs.remove(&subgraph_id);

        true
    }

    /// Add a port to a Group Input or Group Output node.
    /// Create a new individual IO node inside the current subgraph.
    /// Also adds the corresponding port on the parent graph's group node.
    /// Returns the new IO node's EntityId, or None on failure.
    pub fn add_group_io_node(&mut self, kind: GroupIOKind, socket_type: SocketType, label: &str) -> Option<EntityId> {
        let current_id = self.current_graph_id;
        if current_id == self.root_graph_id { return None; }

        let (parent_id, group_node_id) = self.find_parent_group(current_id)?;

        let title = match &kind {
            GroupIOKind::Input => format!("In: {}", label),
            GroupIOKind::Output => format!("Out: {}", label),
        };

        // Create IO node in subgraph with single port
        let sub = self.graphs.get_mut(&current_id)?;
        let io_node = sub.add_node(&title, (0.0, 0.0));
        sub.world.insert(io_node, kind.clone());
        sub.world.insert(io_node, group::GroupIOLabel(label.to_string()));

        let sub_port = match &kind {
            GroupIOKind::Input => sub.add_port(io_node, PortDirection::Output, socket_type, label),
            GroupIOKind::Output => sub.add_port(io_node, PortDirection::Input, socket_type, label),
        };

        // Add corresponding port on parent group node
        let parent = self.graphs.get_mut(&parent_id)?;
        let parent_port = match &kind {
            GroupIOKind::Input => parent.add_port(group_node_id, PortDirection::Input, socket_type, label),
            GroupIOKind::Output => parent.add_port(group_node_id, PortDirection::Output, socket_type, label),
        };

        self.io_port_mapping.insert((current_id, sub_port), parent_port);
        Some(io_node)
    }

    /// After a connection is made to a Group IO port, adapt its type to match
    /// the connected port and update the corresponding parent group node port.
    pub fn adapt_group_io_port(&mut self, io_port_id: EntityId, new_type: SocketType) {
        let current_id = self.current_graph_id;
        if current_id == self.root_graph_id { return; }

        // Update the port type in the subgraph
        if let Some(sub) = self.graphs.get_mut(&current_id) {
            sub.world.insert(io_port_id, PortSocketType(new_type));
        }

        // Find the corresponding parent port via the explicit mapping
        let parent_port = match self.io_port_mapping.get(&(current_id, io_port_id)) {
            Some(&p) => p,
            None => return, // not a mapped IO port
        };

        let (parent_id, _) = match self.find_parent_group(current_id) {
            Some(v) => v,
            None => return,
        };

        if let Some(parent) = self.graphs.get_mut(&parent_id) {
            parent.world.insert(parent_port, PortSocketType(new_type));
        }
    }

    /// Find which parent graph and group node own a given subgraph. O(1) via cache.
    pub fn find_parent_group(&self, subgraph_id: EntityId) -> Option<(EntityId, EntityId)> {
        self.subgraph_parents.get(&subgraph_id).copied()
    }

    /// Get a label for a graph (for breadcrumb display). O(1) via cache.
    pub fn graph_label(&self, graph_id: EntityId) -> String {
        if graph_id == self.root_graph_id {
            return "Root".to_string();
        }
        if let Some(&(parent_id, group_node_id)) = self.subgraph_parents.get(&graph_id) {
            if let Some(parent) = self.graphs.get(&parent_id) {
                return parent.world.get::<NodeHeader>(group_node_id)
                    .map(|h| h.title.clone())
                    .unwrap_or_else(|| "Group".to_string());
            }
        }
        "Group".to_string()
    }

    /// Serialize the entire editor including all subgraphs.
    pub fn serialize_editor(&self) -> crate::serialization::SerializedGraphEditor {
        let mut graphs = std::collections::HashMap::new();
        for (&graph_id, graph) in &self.graphs {
            graphs.insert(graph_id.index, graph.serialize());
        }
        crate::serialization::SerializedGraphEditor {
            root_graph_id: self.root_graph_id.index,
            graphs,
            next_graph_id: self.next_graph_id,
        }
    }

    /// Deserialize a full editor with subgraph hierarchy.
    pub fn deserialize_editor(data: &crate::serialization::SerializedGraphEditor) -> Result<Self, crate::serialization::DeserializeError> {
        use crate::graph::port::PortDirection;

        let mut graphs: HashMap<EntityId, NodeGraph> = HashMap::new();
        let mut graph_id_map: HashMap<u32, EntityId> = HashMap::new();
        // Per-graph entity ID mappings: old serialized ID → new EntityId
        let mut per_graph_id_maps: HashMap<u32, HashMap<u32, EntityId>> = HashMap::new();

        for (&old_graph_id, sgraph) in &data.graphs {
            let (graph, id_map) = NodeGraph::deserialize_with_id_map(sgraph)?;
            let new_graph_id = EntityId { index: old_graph_id, generation: Default::default() };
            graphs.insert(new_graph_id, graph);
            graph_id_map.insert(old_graph_id, new_graph_id);
            per_graph_id_maps.insert(old_graph_id, id_map);
        }

        let root_graph_id = *graph_id_map.get(&data.root_graph_id)
            .unwrap_or(&EntityId { index: data.root_graph_id, generation: Default::default() });

        // Restore SubgraphRoot using serialized node IDs (deterministic, no heuristics)
        let mut subgraph_parents: HashMap<EntityId, (EntityId, EntityId)> = HashMap::new();

        for (&old_graph_id, sgraph) in &data.graphs {
            let parent_graph_id = *graph_id_map.get(&old_graph_id).unwrap();

            if let Some(id_map) = per_graph_id_maps.get(&old_graph_id) {
                for snode in &sgraph.nodes {
                    if let Some(old_subgraph_id) = snode.subgraph_id {
                        if let Some(&new_subgraph_id) = graph_id_map.get(&old_subgraph_id) {
                            // Look up the new EntityId for this node using the exact serialized ID
                            if let Some(&new_node_id) = id_map.get(&snode.id) {
                                subgraph_parents.insert(new_subgraph_id, (parent_graph_id, new_node_id));
                            }
                        }
                    }
                }
            }
        }

        // Insert SubgraphRoot components
        for (&subgraph_id, &(parent_id, group_node_id)) in &subgraph_parents {
            if let Some(graph) = graphs.get_mut(&parent_id) {
                graph.world.insert(group_node_id, group::SubgraphRoot(subgraph_id));
            }
        }

        // Rebuild io_port_mapping (keyed by (subgraph_id, io_port_id))
        let mut io_port_mapping: HashMap<(EntityId, EntityId), EntityId> = HashMap::new();

        for (&subgraph_id, &(parent_id, group_node_id)) in &subgraph_parents {
            let parent_graph = graphs.get(&parent_id);
            let sub_graph = graphs.get(&subgraph_id);
            if let (Some(parent), Some(sub)) = (parent_graph, sub_graph) {
                let group_ports = parent.node_ports(group_node_id).to_vec();
                let group_inputs: Vec<EntityId> = group_ports.iter().filter(|&&pid| {
                    parent.world.get::<PortDirection>(pid).copied() == Some(PortDirection::Input)
                }).copied().collect();
                let group_outputs: Vec<EntityId> = group_ports.iter().filter(|&&pid| {
                    parent.world.get::<PortDirection>(pid).copied() == Some(PortDirection::Output)
                }).copied().collect();

                let mut input_io_ports: Vec<EntityId> = Vec::new();
                let mut output_io_ports: Vec<EntityId> = Vec::new();
                for (nid, kind) in sub.world.query::<GroupIOKind>() {
                    for &pid in sub.node_ports(nid) {
                        match kind {
                            GroupIOKind::Input => input_io_ports.push(pid),
                            GroupIOKind::Output => output_io_ports.push(pid),
                        }
                    }
                }

                for (i, &io_port) in input_io_ports.iter().enumerate() {
                    if let Some(&group_port) = group_inputs.get(i) {
                        io_port_mapping.insert((subgraph_id, io_port), group_port);
                    }
                }
                for (i, &io_port) in output_io_ports.iter().enumerate() {
                    if let Some(&group_port) = group_outputs.get(i) {
                        io_port_mapping.insert((subgraph_id, io_port), group_port);
                    }
                }
            }
        }

        Ok(GraphEditor {
            graphs,
            root_graph_id,
            current_graph_id: root_graph_id,
            breadcrumb: vec![root_graph_id],
            next_graph_id: data.next_graph_id,
            next_entity_start: data.next_graph_id as u32 * 10000 + 10000,
            subgraph_parents,
            io_port_mapping,
        })
    }
}

impl Default for GraphEditor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod graph_editor_tests {
    use super::*;
    use crate::types::socket_type::SocketType;

    fn build_chain(ge: &mut GraphEditor) -> (EntityId, EntityId, EntityId) {
        let g = ge.current_graph_mut();
        let n1 = g.add_node("A", (0.0, 0.0));
        let n1_out = g.add_port(n1, PortDirection::Output, SocketType::Float, "Out");
        let n2 = g.add_node("B", (200.0, 0.0));
        let n2_in = g.add_port(n2, PortDirection::Input, SocketType::Float, "In");
        let n2_out = g.add_port(n2, PortDirection::Output, SocketType::Float, "Out");
        let n3 = g.add_node("C", (400.0, 0.0));
        let n3_in = g.add_port(n3, PortDirection::Input, SocketType::Float, "In");
        g.connect(n1_out, n2_in).unwrap();
        g.connect(n2_out, n3_in).unwrap();
        (n1, n2, n3)
    }

    #[test]
    fn new_graph_editor_has_root() {
        let ge = GraphEditor::new();
        assert_eq!(ge.current_graph_id(), ge.root_graph_id());
        assert_eq!(ge.breadcrumb().len(), 1);
        assert_eq!(ge.current_graph().node_count(), 0);
    }

    #[test]
    fn group_middle_node() {
        let mut ge = GraphEditor::new();
        let (_n1, n2, _n3) = build_chain(&mut ge);

        // Group just n2 (the middle node)
        let result = ge.group_nodes(&[n2]);
        assert!(result.is_some());
        let (group_node, subgraph_id) = result.unwrap();

        // Parent should have: n1, n3, group_node (3 nodes)
        let parent = ge.current_graph();
        assert_eq!(parent.node_count(), 3);

        // Group node should have SubgraphRoot
        assert!(parent.world.get::<SubgraphRoot>(group_node).is_some());

        // Subgraph should have: Group Input + Group Output + the moved node B (3 nodes)
        let sub = ge.graph(subgraph_id).unwrap();
        assert_eq!(sub.node_count(), 3, "Subgraph should have IO nodes + moved node B");

        // Subgraph should have internal connections:
        // Group Input → B's input, B's output → Group Output
        assert_eq!(sub.connection_count(), 2, "Subgraph should have IO→B and B→IO connections");

        // Verify each IO node has exactly 1 port and GroupIOKind
        for (nid, kind) in sub.world.query::<GroupIOKind>() {
            let ports = sub.node_ports(nid);
            assert_eq!(ports.len(), 1, "IO node {:?} ({:?}) should have 1 port, got {}", nid, kind, ports.len());
            let header = sub.world.get::<NodeHeader>(nid).unwrap();
            assert!(header.title.starts_with("In:") || header.title.starts_with("Out:"),
                "IO node title should start with In:/Out:, got '{}'", header.title);
        }

        // Verify moved node B is NOT marked as GroupIOKind and has original ports
        for (nid, header) in sub.world.query::<NodeHeader>() {
            if sub.world.get::<GroupIOKind>(nid).is_none() {
                assert_eq!(header.title, "B", "Non-IO node should be B, got '{}'", header.title);
                let ports = sub.node_ports(nid);
                assert_eq!(ports.len(), 2, "Node B should have 2 ports (In + Out), got {}", ports.len());
            }
        }
    }

    #[test]
    fn enter_and_exit_group() {
        let mut ge = GraphEditor::new();
        let (_n1, n2, _n3) = build_chain(&mut ge);

        let (group_node, subgraph_id) = ge.group_nodes(&[n2]).unwrap();

        // Enter group
        assert!(ge.enter_group(group_node));
        assert_eq!(ge.current_graph_id(), subgraph_id);
        assert_eq!(ge.breadcrumb().len(), 2);

        // Exit group
        assert!(ge.exit_group());
        assert_eq!(ge.current_graph_id(), ge.root_graph_id());
        assert_eq!(ge.breadcrumb().len(), 1);
    }

    #[test]
    fn navigate_to_root() {
        let mut ge = GraphEditor::new();
        let (_n1, n2, _n3) = build_chain(&mut ge);
        let root = ge.root_graph_id();

        let (group_node, _) = ge.group_nodes(&[n2]).unwrap();
        ge.enter_group(group_node);

        assert!(ge.navigate_to(root));
        assert_eq!(ge.current_graph_id(), root);
        assert_eq!(ge.breadcrumb().len(), 1);
    }

    #[test]
    fn ungroup() {
        let mut ge = GraphEditor::new();
        let (_n1, n2, _n3) = build_chain(&mut ge);

        let (group_node, _) = ge.group_nodes(&[n2]).unwrap();
        assert_eq!(ge.current_graph().node_count(), 3);

        assert!(ge.ungroup(group_node));
        // Group node dissolved, B restored: A, B, C all in parent
        assert_eq!(ge.current_graph().node_count(), 3);
        // Connections should be restored too
        assert_eq!(ge.current_graph().connection_count(), 2);
    }

    #[test]
    fn graph_label() {
        let mut ge = GraphEditor::new();
        let root = ge.root_graph_id();
        assert_eq!(ge.graph_label(root), "Root");

        let (_n1, n2, _n3) = build_chain(&mut ge);
        let (_, subgraph_id) = ge.group_nodes(&[n2]).unwrap();
        assert_eq!(ge.graph_label(subgraph_id), "Group");
    }

    #[test]
    fn add_group_io_node() {
        let mut ge = GraphEditor::new();
        let (_n1, n2, _n3) = build_chain(&mut ge);
        let (group_node, _subgraph_id) = ge.group_nodes(&[n2]).unwrap();

        ge.enter_group(group_node);

        // Count IO nodes before
        let io_count_before = ge.current_graph().world.query::<GroupIOKind>().count();

        // Add a new input IO node
        let result = ge.add_group_io_node(GroupIOKind::Input, SocketType::Color, "Custom");
        assert!(result.is_some());

        // Should have one more IO node
        let io_count_after = ge.current_graph().world.query::<GroupIOKind>().count();
        assert_eq!(io_count_after, io_count_before + 1);

        // The new IO node should have exactly 1 port
        let new_io = result.unwrap();
        assert_eq!(ge.current_graph().node_ports(new_io).len(), 1);

        // Exit and check the group node in parent gained a port
        ge.exit_group();
        let parent = ge.current_graph();
        let group_ports = parent.node_ports(group_node);
        assert!(group_ports.len() > 2, "Group node should have gained a port");
    }

    #[test]
    fn enter_nonexistent_group_fails() {
        let mut ge = GraphEditor::new();
        let fake = EntityId { index: 999, generation: crate::store::Generation::default() };
        assert!(!ge.enter_group(fake));
    }

    #[test]
    fn exit_at_root_fails() {
        let mut ge = GraphEditor::new();
        assert!(!ge.exit_group());
    }
}
