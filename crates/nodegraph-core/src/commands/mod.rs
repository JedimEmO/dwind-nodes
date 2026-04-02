use crate::graph::NodeGraph;
use crate::graph::node::{NodeHeader, NodePosition, MuteState};
use crate::graph::port::PortDirection;
use crate::graph::connection::ConnectionEndpoints;
use crate::store::EntityId;
use crate::types::socket_type::SocketType;
use crate::serialization::{SerializedGraph, SerializedNode, SerializedPort, SerializedConnection};

use std::fmt;

// ============================================================
// Command trait and history
// ============================================================

pub trait Command: fmt::Debug {
    fn execute(&mut self, graph: &mut NodeGraph);
    fn undo(&mut self, graph: &mut NodeGraph);
    fn description(&self) -> &str;
}

pub struct CommandHistory {
    undo_stack: Vec<Box<dyn Command>>,
    redo_stack: Vec<Box<dyn Command>>,
}

impl CommandHistory {
    pub fn new() -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    pub fn execute(&mut self, mut command: Box<dyn Command>, graph: &mut NodeGraph) {
        command.execute(graph);
        self.undo_stack.push(command);
        self.redo_stack.clear();
    }

    pub fn undo(&mut self, graph: &mut NodeGraph) -> bool {
        if let Some(mut command) = self.undo_stack.pop() {
            command.undo(graph);
            self.redo_stack.push(command);
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self, graph: &mut NodeGraph) -> bool {
        if let Some(mut command) = self.redo_stack.pop() {
            command.execute(graph);
            self.undo_stack.push(command);
            true
        } else {
            false
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Push a command that was already executed externally onto the undo stack.
    /// Used when the InteractionController directly mutates the graph (e.g., node drag)
    /// and we want to record it for undo without re-executing.
    pub fn push_already_executed(&mut self, command: Box<dyn Command>) {
        self.undo_stack.push(command);
        self.redo_stack.clear();
    }

    pub fn undo_count(&self) -> usize {
        self.undo_stack.len()
    }

    pub fn redo_count(&self) -> usize {
        self.redo_stack.len()
    }
}

impl Default for CommandHistory {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================
// MoveNodesCommand
// ============================================================

#[derive(Debug)]
pub struct MoveNodesCommand {
    pub node_ids: Vec<EntityId>,
    pub delta_x: f64,
    pub delta_y: f64,
}

impl Command for MoveNodesCommand {
    fn execute(&mut self, graph: &mut NodeGraph) {
        for &id in &self.node_ids {
            if let Some(pos) = graph.world.get_mut::<NodePosition>(id) {
                pos.x += self.delta_x;
                pos.y += self.delta_y;
            }
        }
    }

    fn undo(&mut self, graph: &mut NodeGraph) {
        for &id in &self.node_ids {
            if let Some(pos) = graph.world.get_mut::<NodePosition>(id) {
                pos.x -= self.delta_x;
                pos.y -= self.delta_y;
            }
        }
    }

    fn description(&self) -> &str {
        "Move nodes"
    }
}

// ============================================================
// AddNodeCommand
// ============================================================

#[derive(Debug)]
pub struct AddNodeCommand {
    pub title: String,
    pub position: (f64, f64),
    pub ports: Vec<(PortDirection, SocketType, String)>,
    /// Populated after execute
    pub node_id: Option<EntityId>,
    pub port_ids: Vec<EntityId>,
}

impl AddNodeCommand {
    pub fn new(title: &str, position: (f64, f64), ports: Vec<(PortDirection, SocketType, String)>) -> Self {
        Self {
            title: title.to_string(),
            position,
            ports,
            node_id: None,
            port_ids: Vec::new(),
        }
    }
}

impl Command for AddNodeCommand {
    fn execute(&mut self, graph: &mut NodeGraph) {
        let node_id = graph.add_node(&self.title, self.position);
        self.port_ids.clear();
        for (dir, socket_type, label) in &self.ports {
            let port_id = graph.add_port(node_id, *dir, *socket_type, label);
            self.port_ids.push(port_id);
        }
        self.node_id = Some(node_id);
    }

    fn undo(&mut self, graph: &mut NodeGraph) {
        if let Some(node_id) = self.node_id {
            graph.remove_node(node_id);
            self.node_id = None;
            self.port_ids.clear();
        }
    }

    fn description(&self) -> &str {
        "Add node"
    }
}

// ============================================================
// RemoveNodeCommand
// ============================================================

#[derive(Debug)]
pub struct RemoveNodeCommand {
    pub node_id: EntityId,
    /// Snapshot for undo — populated on execute
    snapshot: Option<NodeSnapshot>,
}

#[derive(Debug, Clone)]
struct NodeSnapshot {
    header: NodeHeader,
    position: NodePosition,
    ports: Vec<PortSnapshot>,
    connections: Vec<ConnectionSnapshot>,
}

#[derive(Debug, Clone)]
struct PortSnapshot {
    direction: PortDirection,
    socket_type: SocketType,
    label: String,
}

#[derive(Debug, Clone)]
struct ConnectionSnapshot {
    /// If the other end is external, store its EntityId
    source_external: Option<EntityId>,
    target_external: Option<EntityId>,
    /// Index into the port snapshot list for the local port involved
    local_port_index: usize,
}

impl RemoveNodeCommand {
    pub fn new(node_id: EntityId) -> Self {
        Self { node_id, snapshot: None }
    }

    fn snapshot_node(graph: &NodeGraph, node_id: EntityId) -> Option<NodeSnapshot> {
        use crate::graph::port::{PortSocketType, PortLabel};

        let header = graph.world.get::<NodeHeader>(node_id)?.clone();
        let position = graph.world.get::<NodePosition>(node_id)?.clone();

        let port_ids = graph.node_ports(node_id);
        let mut ports = Vec::new();
        for &pid in port_ids {
            let dir = *graph.world.get::<PortDirection>(pid)?;
            let st = graph.world.get::<PortSocketType>(pid)?.0;
            let label = graph.world.get::<PortLabel>(pid)?.0.clone();
            ports.push(PortSnapshot { direction: dir, socket_type: st, label });
        }

        let mut connections = Vec::new();
        let mut seen_conn_ids = std::collections::HashSet::new();
        for (port_index, &pid) in port_ids.iter().enumerate() {
            for &conn_id in graph.port_connections(pid) {
                if !seen_conn_ids.insert(conn_id) { continue; }
                let endpoints = graph.world.get::<ConnectionEndpoints>(conn_id)?;
                let src_external = if !port_ids.contains(&endpoints.source_port) {
                    Some(endpoints.source_port)
                } else { None };
                let tgt_external = if !port_ids.contains(&endpoints.target_port) {
                    Some(endpoints.target_port)
                } else { None };
                connections.push(ConnectionSnapshot {
                    source_external: src_external,
                    target_external: tgt_external,
                    local_port_index: port_index,
                });
            }
        }

        Some(NodeSnapshot { header, position, ports, connections })
    }
}

impl Command for RemoveNodeCommand {
    fn execute(&mut self, graph: &mut NodeGraph) {
        self.snapshot = Self::snapshot_node(graph, self.node_id);
        graph.remove_node(self.node_id);
    }

    fn undo(&mut self, graph: &mut NodeGraph) {
        if let Some(ref snap) = self.snapshot {
            let node_id = graph.add_node(&snap.header.title, (snap.position.x, snap.position.y));
            if let Some(h) = graph.world.get_mut::<NodeHeader>(node_id) {
                h.color = snap.header.color;
                h.collapsed = snap.header.collapsed;
            }
            self.node_id = node_id;

            for ps in &snap.ports {
                graph.add_port(node_id, ps.direction, ps.socket_type, &ps.label);
            }

            // Reconnect external connections using saved port index
            let new_ports = graph.node_ports(node_id).to_vec();
            for cs in &snap.connections {
                if cs.local_port_index >= new_ports.len() { continue; }
                let local_port = new_ports[cs.local_port_index];
                if let Some(src_ext) = cs.source_external {
                    let _ = graph.connect(src_ext, local_port);
                }
                if let Some(tgt_ext) = cs.target_external {
                    let _ = graph.connect(local_port, tgt_ext);
                }
            }
        }
    }

    fn description(&self) -> &str {
        "Remove node"
    }
}

// ============================================================
// ConnectCommand
// ============================================================

#[derive(Debug)]
pub struct ConnectCommand {
    pub source_port: EntityId,
    pub target_port: EntityId,
    pub connection_id: Option<EntityId>,
    /// If the target input already had a connection, store it for undo
    replaced_connection: Option<(EntityId, EntityId)>, // (source, target) of replaced
}

impl ConnectCommand {
    pub fn new(source_port: EntityId, target_port: EntityId) -> Self {
        Self {
            source_port,
            target_port,
            connection_id: None,
            replaced_connection: None,
        }
    }
}

impl Command for ConnectCommand {
    fn execute(&mut self, graph: &mut NodeGraph) {
        // Check if target input already has a connection
        let target_dir = graph.world.get::<PortDirection>(self.target_port).copied();
        let input_port = if target_dir == Some(PortDirection::Input) {
            self.target_port
        } else {
            self.source_port
        };

        let existing_conns = graph.port_connections(input_port).to_vec();
        if let Some(&existing_conn) = existing_conns.first() {
            if let Some(ep) = graph.world.get::<ConnectionEndpoints>(existing_conn) {
                self.replaced_connection = Some((ep.source_port, ep.target_port));
            }
        }

        match graph.connect(self.source_port, self.target_port) {
            Ok(conn_id) => self.connection_id = Some(conn_id),
            Err(_) => {}
        }
    }

    fn undo(&mut self, graph: &mut NodeGraph) {
        if let Some(conn_id) = self.connection_id {
            graph.disconnect(conn_id);
            self.connection_id = None;
        }
        // Restore replaced connection
        if let Some((src, tgt)) = self.replaced_connection.take() {
            let _ = graph.connect(src, tgt);
        }
    }

    fn description(&self) -> &str {
        "Connect ports"
    }
}

// ============================================================
// DisconnectCommand
// ============================================================

#[derive(Debug)]
pub struct DisconnectCommand {
    pub connection_id: EntityId,
    endpoints: Option<(EntityId, EntityId)>,
}

impl DisconnectCommand {
    pub fn new(connection_id: EntityId) -> Self {
        Self { connection_id, endpoints: None }
    }
}

impl Command for DisconnectCommand {
    fn execute(&mut self, graph: &mut NodeGraph) {
        if let Some(ep) = graph.world.get::<ConnectionEndpoints>(self.connection_id) {
            self.endpoints = Some((ep.source_port, ep.target_port));
        }
        graph.disconnect(self.connection_id);
    }

    fn undo(&mut self, graph: &mut NodeGraph) {
        if let Some((src, tgt)) = self.endpoints {
            if let Ok(conn_id) = graph.connect(src, tgt) {
                self.connection_id = conn_id;
            }
        }
    }

    fn description(&self) -> &str {
        "Disconnect"
    }
}

// ============================================================
// MuteNodeCommand
// ============================================================

#[derive(Debug)]
pub struct MuteNodeCommand {
    pub node_id: EntityId,
    pub muted: bool,
}

impl Command for MuteNodeCommand {
    fn execute(&mut self, graph: &mut NodeGraph) {
        graph.world.insert(self.node_id, MuteState(self.muted));
    }

    fn undo(&mut self, graph: &mut NodeGraph) {
        graph.world.insert(self.node_id, MuteState(!self.muted));
    }

    fn description(&self) -> &str {
        "Toggle mute"
    }
}

// ============================================================
// CollapseNodeCommand
// ============================================================

#[derive(Debug)]
pub struct CollapseNodeCommand {
    pub node_id: EntityId,
    pub collapsed: bool,
}

impl Command for CollapseNodeCommand {
    fn execute(&mut self, graph: &mut NodeGraph) {
        if let Some(h) = graph.world.get_mut::<NodeHeader>(self.node_id) {
            h.collapsed = self.collapsed;
        }
    }

    fn undo(&mut self, graph: &mut NodeGraph) {
        if let Some(h) = graph.world.get_mut::<NodeHeader>(self.node_id) {
            h.collapsed = !self.collapsed;
        }
    }

    fn description(&self) -> &str {
        "Toggle collapse"
    }
}

// ============================================================
// DuplicateNodesCommand
// ============================================================

#[derive(Debug)]
pub struct DuplicateNodesCommand {
    pub source_ids: Vec<EntityId>,
    pub offset: (f64, f64),
    /// Populated after execute
    pub new_node_ids: Vec<EntityId>,
}

impl DuplicateNodesCommand {
    pub fn new(source_ids: Vec<EntityId>, offset: (f64, f64)) -> Self {
        Self { source_ids, offset, new_node_ids: Vec::new() }
    }
}

impl Command for DuplicateNodesCommand {
    fn execute(&mut self, graph: &mut NodeGraph) {
        use crate::graph::port::{PortSocketType, PortLabel};
        use std::collections::HashMap;

        let mut port_map: HashMap<EntityId, EntityId> = HashMap::new();
        self.new_node_ids.clear();

        // Duplicate each node
        for &src_node in &self.source_ids {
            let header = match graph.world.get::<NodeHeader>(src_node) {
                Some(h) => h.clone(),
                None => continue,
            };
            let pos = match graph.world.get::<NodePosition>(src_node) {
                Some(p) => p.clone(),
                None => continue,
            };

            let new_node = graph.add_node(&header.title, (pos.x + self.offset.0, pos.y + self.offset.1));
            if let Some(h) = graph.world.get_mut::<NodeHeader>(new_node) {
                h.color = header.color;
                h.collapsed = header.collapsed;
            }
            // Copy mute state
            if let Some(ms) = graph.world.get::<MuteState>(src_node).cloned() {
                graph.world.insert(new_node, ms);
            }

            self.new_node_ids.push(new_node);

            // Duplicate ports
            let old_ports = graph.node_ports(src_node).to_vec();
            for old_port in old_ports {
                let dir = match graph.world.get::<PortDirection>(old_port) {
                    Some(d) => *d,
                    None => continue,
                };
                let st = match graph.world.get::<PortSocketType>(old_port) {
                    Some(s) => s.0,
                    None => continue,
                };
                let label = match graph.world.get::<PortLabel>(old_port) {
                    Some(l) => l.0.clone(),
                    None => continue,
                };
                let new_port = graph.add_port(new_node, dir, st, &label);
                port_map.insert(old_port, new_port);
            }
        }

        // Duplicate internal connections (connections between duplicated nodes)
        let source_set: std::collections::HashSet<EntityId> = self.source_ids.iter().copied().collect();
        let mut duplicated_connections = Vec::new();

        for &src_node in &self.source_ids {
            let old_ports = graph.node_ports(src_node).to_vec();
            for old_port in old_ports {
                for &conn_id in graph.port_connections(old_port) {
                    if let Some(ep) = graph.world.get::<ConnectionEndpoints>(conn_id) {
                        let src_owner = graph.world.get::<PortOwner>(ep.source_port).map(|o| o.0);
                        let tgt_owner = graph.world.get::<PortOwner>(ep.target_port).map(|o| o.0);
                        // Only duplicate if both endpoints are in the source set
                        if let (Some(so), Some(to)) = (src_owner, tgt_owner) {
                            if source_set.contains(&so) && source_set.contains(&to) {
                                let pair = (ep.source_port, ep.target_port);
                                if !duplicated_connections.contains(&pair) {
                                    duplicated_connections.push(pair);
                                }
                            }
                        }
                    }
                }
            }
        }

        for (old_src, old_tgt) in duplicated_connections {
            if let (Some(&new_src), Some(&new_tgt)) = (port_map.get(&old_src), port_map.get(&old_tgt)) {
                let _ = graph.connect(new_src, new_tgt);
            }
        }
    }

    fn undo(&mut self, graph: &mut NodeGraph) {
        for &new_node in self.new_node_ids.iter().rev() {
            graph.remove_node(new_node);
        }
        self.new_node_ids.clear();
    }

    fn description(&self) -> &str {
        "Duplicate nodes"
    }
}

// ============================================================
// Clipboard (copy/paste as serialize/deserialize)
// ============================================================

use crate::graph::port::{PortOwner, PortSocketType, PortLabel};

pub fn copy_nodes(graph: &NodeGraph, node_ids: &[EntityId]) -> SerializedGraph {
    let source_set: std::collections::HashSet<EntityId> = node_ids.iter().copied().collect();
    let mut nodes = Vec::new();
    let mut connections = Vec::new();
    let mut seen_connections = std::collections::HashSet::new();

    for &node_id in node_ids {
        let header = match graph.world.get::<NodeHeader>(node_id) {
            Some(h) => h.clone(),
            None => continue,
        };
        let pos = graph.world.get::<NodePosition>(node_id)
            .map(|p| (p.x, p.y))
            .unwrap_or((0.0, 0.0));

        let port_ids = graph.node_ports(node_id);
        let mut ports = Vec::new();
        for &pid in port_ids {
            let dir = graph.world.get::<PortDirection>(pid).copied().unwrap_or(PortDirection::Input);
            let st = graph.world.get::<PortSocketType>(pid).map(|s| s.0).unwrap_or(SocketType::Float);
            let label = graph.world.get::<PortLabel>(pid).map(|l| l.0.clone()).unwrap_or_default();
            ports.push(SerializedPort { id: pid.index, direction: dir, socket_type: st, label });

            // Collect internal connections
            for &conn_id in graph.port_connections(pid) {
                if seen_connections.contains(&conn_id.index) { continue; }
                if let Some(ep) = graph.world.get::<ConnectionEndpoints>(conn_id) {
                    let src_owner = graph.world.get::<PortOwner>(ep.source_port).map(|o| o.0);
                    let tgt_owner = graph.world.get::<PortOwner>(ep.target_port).map(|o| o.0);
                    if let (Some(so), Some(to)) = (src_owner, tgt_owner) {
                        if source_set.contains(&so) && source_set.contains(&to) {
                            connections.push(SerializedConnection {
                                id: conn_id.index,
                                source_port: ep.source_port.index,
                                target_port: ep.target_port.index,
                            });
                            seen_connections.insert(conn_id.index);
                        }
                    }
                }
            }
        }

        nodes.push(SerializedNode { id: node_id.index, header, position: pos, ports });
    }

    SerializedGraph { nodes, connections }
}

pub fn paste_nodes(graph: &mut NodeGraph, data: &SerializedGraph, offset: (f64, f64)) -> Vec<EntityId> {
    use std::collections::HashMap;

    let mut id_map: HashMap<u32, EntityId> = HashMap::new();
    let mut new_nodes = Vec::new();

    for snode in &data.nodes {
        let node_id = graph.add_node(
            &snode.header.title,
            (snode.position.0 + offset.0, snode.position.1 + offset.1),
        );
        if let Some(h) = graph.world.get_mut::<NodeHeader>(node_id) {
            h.color = snode.header.color;
            h.collapsed = snode.header.collapsed;
        }
        id_map.insert(snode.id, node_id);
        new_nodes.push(node_id);

        for sport in &snode.ports {
            let port_id = graph.add_port(node_id, sport.direction, sport.socket_type, &sport.label);
            id_map.insert(sport.id, port_id);
        }
    }

    for sconn in &data.connections {
        if let (Some(&src), Some(&tgt)) = (id_map.get(&sconn.source_port), id_map.get(&sconn.target_port)) {
            let _ = graph.connect(src, tgt);
        }
    }

    new_nodes
}

#[cfg(test)]
mod tests;
