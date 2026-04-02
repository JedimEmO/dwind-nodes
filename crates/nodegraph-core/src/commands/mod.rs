use crate::graph::GraphEditor;

/// Snapshot-based undo/redo history.
/// Each undoable action pushes the entire GraphEditor state before mutation.
/// Undo = restore previous snapshot. Redo = restore next snapshot.
/// Correct by construction: no stale entity IDs, no inverse logic bugs.
pub struct UndoHistory {
    undo_stack: Vec<GraphEditor>,
    redo_stack: Vec<GraphEditor>,
    max_entries: usize,
}

impl UndoHistory {
    pub fn new() -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_entries: 50,
        }
    }

    /// Snapshot the current state before an undoable mutation.
    /// Call this BEFORE modifying the editor.
    pub fn save(&mut self, editor: &GraphEditor) {
        self.undo_stack.push(editor.clone());
        self.redo_stack.clear();
        // Limit memory
        if self.undo_stack.len() > self.max_entries {
            self.undo_stack.remove(0);
        }
    }

    /// Undo: restore the previous state.
    /// Pushes the current state onto the redo stack.
    pub fn undo(&mut self, editor: &mut GraphEditor) -> bool {
        if let Some(prev) = self.undo_stack.pop() {
            self.redo_stack.push(editor.clone());
            *editor = prev;
            true
        } else {
            false
        }
    }

    /// Redo: restore the next state.
    /// Pushes the current state onto the undo stack.
    pub fn redo(&mut self, editor: &mut GraphEditor) -> bool {
        if let Some(next) = self.redo_stack.pop() {
            self.undo_stack.push(editor.clone());
            *editor = next;
            true
        } else {
            false
        }
    }

    pub fn can_undo(&self) -> bool { !self.undo_stack.is_empty() }
    pub fn can_redo(&self) -> bool { !self.redo_stack.is_empty() }
}

impl Default for UndoHistory {
    fn default() -> Self { Self::new() }
}

// Keep clipboard functions — they don't need Command types
use crate::graph::NodeGraph;
use crate::graph::node::NodeHeader;
use crate::graph::port::{PortDirection, PortOwner, PortSocketType, PortLabel};
use crate::graph::connection::ConnectionEndpoints;
use crate::store::EntityId;
use crate::types::socket_type::SocketType;
use crate::serialization::{SerializedGraph, SerializedNode, SerializedPort, SerializedConnection};

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
        let pos = graph.world.get::<crate::graph::node::NodePosition>(node_id)
            .map(|p| (p.x, p.y)).unwrap_or((0.0, 0.0));

        let port_ids = graph.node_ports(node_id);
        let mut ports = Vec::new();
        for &pid in port_ids {
            let dir = graph.world.get::<PortDirection>(pid).copied().unwrap_or(PortDirection::Input);
            let st = graph.world.get::<PortSocketType>(pid).map(|s| s.0).unwrap_or(SocketType::Float);
            let label = graph.world.get::<PortLabel>(pid).map(|l| l.0.clone()).unwrap_or_default();
            ports.push(SerializedPort { id: pid.index, direction: dir, socket_type: st, label });

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
        let is_reroute = graph.world.get::<crate::graph::reroute::IsReroute>(node_id).is_some();
        nodes.push(SerializedNode { id: node_id.index, header, position: pos, ports, is_reroute });
    }
    SerializedGraph { nodes, connections }
}

pub fn paste_nodes(graph: &mut NodeGraph, data: &SerializedGraph, offset: (f64, f64)) -> Vec<EntityId> {
    use std::collections::HashMap;
    let mut id_map: HashMap<u32, EntityId> = HashMap::new();
    let mut new_nodes = Vec::new();

    for snode in &data.nodes {
        let node_id = graph.add_node(&snode.header.title, (snode.position.0 + offset.0, snode.position.1 + offset.1));
        if let Some(h) = graph.world.get_mut::<NodeHeader>(node_id) {
            h.color = snode.header.color;
            h.collapsed = snode.header.collapsed;
        }
        if snode.is_reroute {
            graph.world.insert(node_id, crate::graph::reroute::IsReroute);
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
