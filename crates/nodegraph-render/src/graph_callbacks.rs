use std::collections::HashMap;
use std::rc::Rc;

use nodegraph_core::store::EntityId;

pub type GroupFn = dyn Fn(EntityId, EntityId, HashMap<EntityId, EntityId>);
pub type UngroupFn = dyn Fn(HashMap<EntityId, EntityId>);

/// Event callbacks for graph mutations that carry *ephemeral* data which cannot
/// be derived from the reactive graph state after the fact.
///
/// Most graph observation should use signals instead of callbacks — subscribe
/// to [`crate::graph_signals::GraphSignals::node_list`] /
/// [`crate::graph_signals::GraphSignals::connection_list`] (both `MutableVec`),
/// or per-node `Mutable`s like `node_positions` / `selection`. These re-emit on
/// every mutation and compose cleanly with dominator's `.child_signal*` APIs.
///
/// Only group/ungroup remain as callbacks because the old→new port-ID map is
/// transient: consumers keyed on port IDs (e.g. an external parameter store)
/// need to migrate their state at the moment of the operation, and no
/// steady-state signal can reconstruct that mapping after both graphs change.
#[derive(Default)]
pub struct GraphCallbacks {
    /// Fired after nodes are grouped. Args: (group_node_id, subgraph_id, old_to_new_port_map).
    pub on_group: Option<Rc<GroupFn>>,
    /// Fired after a group is ungrouped. Args: (old_to_new_port_map).
    pub on_ungroup: Option<Rc<UngroupFn>>,
}
