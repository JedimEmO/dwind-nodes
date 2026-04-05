use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use crate::graph::{NodeGraph, ConnectionError, GroupIOKind};
use crate::graph::group::SubgraphRoot;
use crate::store::EntityId;
use crate::types::socket_type::SocketType;
use crate::graph::node::NodeHeader;
use crate::graph::port::PortDirection;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedGraph {
    pub nodes: Vec<SerializedNode>,
    pub connections: Vec<SerializedConnection>,
    #[serde(default)]
    pub frames: Vec<SerializedFrame>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedNode {
    pub id: u32,
    pub header: NodeHeader,
    pub position: (f64, f64),
    pub ports: Vec<SerializedPort>,
    #[serde(default)]
    pub is_reroute: bool,
    #[serde(default)]
    pub subgraph_id: Option<u32>,
    #[serde(default)]
    pub group_io_kind: Option<GroupIOKind>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedPort {
    pub id: u32,
    pub direction: PortDirection,
    pub socket_type: SocketType,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedConnection {
    pub id: u32,
    pub source_port: u32,
    pub target_port: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedFrame {
    pub label: String,
    pub color: [u8; 3],
    pub member_node_ids: Vec<u32>,
}

/// Full graph editor state including subgraph hierarchy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedGraphEditor {
    pub root_graph_id: u32,
    pub graphs: HashMap<u32, SerializedGraph>,
    pub next_graph_id: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DeserializeError {
    OrphanedConnection { source_port: u32, target_port: u32 },
    ConnectionFailed { source_port: u32, target_port: u32, reason: ConnectionError },
}

impl NodeGraph {
    pub fn serialize(&self) -> SerializedGraph {
        use crate::graph::node::NodePosition;
        use crate::graph::port::{PortSocketType, PortLabel};
        use crate::graph::connection::ConnectionEndpoints;
        use crate::graph::frame::{FrameLabel, FrameColor, FrameMembers, FrameRect};

        let mut nodes = Vec::new();
        for (node_id, header) in self.world.query::<NodeHeader>() {
            let pos = self.world.get::<NodePosition>(node_id)
                .map(|p| (p.x, p.y))
                .unwrap_or((0.0, 0.0));

            let port_ids = self.node_ports(node_id);
            let mut ports = Vec::new();
            for &port_id in port_ids {
                let direction = self.world.get::<PortDirection>(port_id)
                    .cloned()
                    .unwrap_or(PortDirection::Input);
                let socket_type = self.world.get::<PortSocketType>(port_id)
                    .map(|s| s.0)
                    .unwrap_or(SocketType::Float);
                let label = self.world.get::<PortLabel>(port_id)
                    .map(|l| l.0.clone())
                    .unwrap_or_default();
                ports.push(SerializedPort {
                    id: port_id.index,
                    direction,
                    socket_type,
                    label,
                });
            }

            let is_reroute = self.world.get::<crate::graph::reroute::IsReroute>(node_id).is_some();
            let subgraph_id = self.world.get::<SubgraphRoot>(node_id).map(|s| s.0.index);
            let group_io_kind = self.world.get::<GroupIOKind>(node_id).cloned();

            nodes.push(SerializedNode {
                id: node_id.index,
                header: header.clone(),
                position: pos,
                ports,
                is_reroute,
                subgraph_id,
                group_io_kind,
            });
        }

        let mut connections = Vec::new();
        for (conn_id, endpoints) in self.world.query::<ConnectionEndpoints>() {
            connections.push(SerializedConnection {
                id: conn_id.index,
                source_port: endpoints.source_port.index,
                target_port: endpoints.target_port.index,
            });
        }

        let mut frames = Vec::new();
        for (frame_id, _) in self.world.query::<FrameRect>() {
            let label = self.world.get::<FrameLabel>(frame_id)
                .map(|l| l.0.clone()).unwrap_or_default();
            let color = self.world.get::<FrameColor>(frame_id)
                .map(|c| c.0).unwrap_or([80, 80, 120]);
            let member_node_ids = self.world.get::<FrameMembers>(frame_id)
                .map(|m| m.0.iter().map(|id| id.index).collect())
                .unwrap_or_default();
            frames.push(SerializedFrame { label, color, member_node_ids });
        }

        SerializedGraph { nodes, connections, frames }
    }

    pub fn deserialize(data: &SerializedGraph) -> Result<Self, DeserializeError> {
        Self::deserialize_with_id_map(data).map(|(graph, _)| graph)
    }

    /// Deserialize and return the old→new entity ID mapping (used by GraphEditor deserialization).
    pub fn deserialize_with_id_map(data: &SerializedGraph) -> Result<(Self, HashMap<u32, EntityId>), DeserializeError> {
        let mut graph = NodeGraph::new();
        let mut id_map: HashMap<u32, EntityId> = HashMap::new();

        for snode in &data.nodes {
            let node_id = graph.add_node(&snode.header.title, (snode.position.0, snode.position.1));
            if let Some(header) = graph.world.get_mut::<NodeHeader>(node_id) {
                header.color = snode.header.color;
                header.collapsed = snode.header.collapsed;
            }
            if snode.is_reroute {
                graph.world.insert(node_id, crate::graph::reroute::IsReroute);
            }
            // SubgraphRoot restored later (needs graph_id mapping)
            // GroupIOKind restored here
            if let Some(ref kind) = snode.group_io_kind {
                graph.world.insert(node_id, kind.clone());
                // Derive GroupIOLabel from title prefix
                let label = if snode.header.title.starts_with("In: ") {
                    snode.header.title.strip_prefix("In: ").unwrap_or("").to_string()
                } else if snode.header.title.starts_with("Out: ") {
                    snode.header.title.strip_prefix("Out: ").unwrap_or("").to_string()
                } else {
                    String::new()
                };
                graph.world.insert(node_id, crate::graph::group::GroupIOLabel(label));
            }
            id_map.insert(snode.id, node_id);

            for sport in &snode.ports {
                let port_id = graph.add_port(node_id, sport.direction, sport.socket_type, &sport.label);
                id_map.insert(sport.id, port_id);
            }
        }

        for sconn in &data.connections {
            match (id_map.get(&sconn.source_port), id_map.get(&sconn.target_port)) {
                (Some(&src), Some(&tgt)) => {
                    graph.connect(src, tgt).map_err(|e| DeserializeError::ConnectionFailed {
                        source_port: sconn.source_port,
                        target_port: sconn.target_port,
                        reason: e,
                    })?;
                }
                _ => {
                    return Err(DeserializeError::OrphanedConnection {
                        source_port: sconn.source_port,
                        target_port: sconn.target_port,
                    });
                }
            }
        }

        for sframe in &data.frames {
            let members: Vec<EntityId> = sframe.member_node_ids.iter()
                .filter_map(|old_id| id_map.get(old_id).copied())
                .collect();
            graph.add_frame(&sframe.label, sframe.color, &members);
        }

        Ok((graph, id_map))
    }
}

// GraphEditor::serialize_editor() and deserialize_editor() are in graph/mod.rs
// (they need access to private GraphEditor fields)
