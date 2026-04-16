pub mod commands;
pub mod graph;
pub mod interaction;
pub mod layout;
pub mod search;
pub mod serialization;
pub mod store;
pub mod types;
pub mod viewport;

// Re-exports for the quick-start API. Components of internal types
// (e.g. NodePosition, NodeTypeId, PortSocketType) remain accessible via
// their full module paths (graph::node, graph::port) but are not
// hoisted to the crate root to keep the surface minimal.
pub use graph::node::NodeHeader;
pub use graph::port::PortDirection;
pub use graph::{ConnectionError, GraphEditor, GroupIOKind, NodeGraph};
pub use search::{NodeTypeDefinition, NodeTypeRegistry, PortDefinition};
pub use serialization::{DeserializeError, SerializedGraph, SerializedGraphEditor};
pub use store::EntityId;
pub use types::socket_type::SocketType;
