pub mod store;
pub mod graph;
pub mod types;
pub mod layout;
pub mod commands;
pub mod viewport;
pub mod interaction;
pub mod search;
pub mod serialization;

// Re-exports for ergonomic imports
pub use store::EntityId;
pub use graph::{NodeGraph, GraphEditor, ConnectionError, GroupIOKind};
pub use graph::node::{NodeHeader, NodePosition, NodeTypeId};
pub use graph::port::PortDirection;
pub use types::socket_type::SocketType;
pub use search::{NodeTypeDefinition, PortDefinition, NodeTypeRegistry};
pub use serialization::{SerializedGraph, SerializedGraphEditor, DeserializeError};
