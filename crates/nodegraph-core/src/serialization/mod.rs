use serde::{Deserialize, Serialize};
use crate::graph::{NodeGraph, ConnectionError};
use crate::store::EntityId;
use crate::types::socket_type::SocketType;
use crate::graph::node::NodeHeader;
use crate::graph::port::PortDirection;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedGraph {
    pub nodes: Vec<SerializedNode>,
    pub connections: Vec<SerializedConnection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedNode {
    pub id: u32,
    pub header: NodeHeader,
    pub position: (f64, f64),
    pub ports: Vec<SerializedPort>,
    #[serde(default)]
    pub is_reroute: bool,
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
            nodes.push(SerializedNode {
                id: node_id.index,
                header: header.clone(),
                position: pos,
                ports,
                is_reroute,
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

        SerializedGraph { nodes, connections }
    }

    pub fn deserialize(data: &SerializedGraph) -> Result<Self, DeserializeError> {
        use std::collections::HashMap;

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

        Ok(graph)
    }
}
