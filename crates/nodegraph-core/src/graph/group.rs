use crate::store::EntityId;
use crate::types::socket_type::SocketType;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SubgraphRoot(pub EntityId);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GroupIOLabel(pub String);

/// A single IO binding between a parent-graph port and the corresponding
/// internal port on a group_input / group_output node in the subgraph.
///
/// Using a named struct instead of parallel vectors or index-matched tuples
/// makes the pairing of external↔internal explicit: downstream code accesses
/// fields by name rather than zipping by position.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupIOBinding {
    /// Port on the parent graph side that was severed by grouping.
    pub external_port: EntityId,
    /// Port on the outside-facing group node, or (during ungroup) the
    /// corresponding internal port being reconnected.
    pub internal_port: EntityId,
    pub socket_type: SocketType,
    pub label: String,
}
