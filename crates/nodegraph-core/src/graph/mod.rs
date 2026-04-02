pub mod node;
pub mod port;
pub mod connection;
pub mod group;
pub mod frame;
pub mod reroute;

use std::collections::HashMap;
use crate::store::{EntityId, World};
use crate::types::socket_type::SocketType;
use node::{NodeHeader, NodePosition};
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
}

impl Default for NodeGraph {
    fn default() -> Self {
        Self::new()
    }
}
