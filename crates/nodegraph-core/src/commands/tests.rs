use super::*;
use crate::graph::NodeGraph;
use crate::graph::node::{NodeHeader, NodePosition, MuteState};
use crate::graph::port::PortDirection;
use crate::graph::connection::ConnectionEndpoints;
use crate::types::socket_type::SocketType;
use crate::store::EntityId;

fn make_two_connected_nodes(graph: &mut NodeGraph) -> (EntityId, EntityId, EntityId, EntityId, EntityId) {
    let n1 = graph.add_node("N1", (0.0, 0.0));
    let n1_out = graph.add_port(n1, PortDirection::Output, SocketType::Float, "Out");
    let n2 = graph.add_node("N2", (200.0, 0.0));
    let n2_in = graph.add_port(n2, PortDirection::Input, SocketType::Float, "In");
    let conn = graph.connect(n1_out, n2_in).unwrap();
    (n1, n1_out, n2, n2_in, conn)
}

// ============================================================
// AC 1: Undo/redo move
// ============================================================

#[test]
fn undo_redo_move() {
    let mut graph = NodeGraph::new();
    let node = graph.add_node("N", (0.0, 0.0));
    let mut history = CommandHistory::new();

    history.execute(
        Box::new(MoveNodesCommand { node_ids: vec![node], delta_x: 100.0, delta_y: 50.0 }),
        &mut graph,
    );

    let pos = graph.world.get::<NodePosition>(node).unwrap();
    assert!((pos.x - 100.0).abs() < 1e-10);
    assert!((pos.y - 50.0).abs() < 1e-10);

    // Undo
    assert!(history.undo(&mut graph));
    let pos = graph.world.get::<NodePosition>(node).unwrap();
    assert!((pos.x - 0.0).abs() < 1e-10);
    assert!((pos.y - 0.0).abs() < 1e-10);

    // Redo
    assert!(history.redo(&mut graph));
    let pos = graph.world.get::<NodePosition>(node).unwrap();
    assert!((pos.x - 100.0).abs() < 1e-10);
    assert!((pos.y - 50.0).abs() < 1e-10);
}

// ============================================================
// AC 2: Undo/redo connect
// ============================================================

#[test]
fn undo_redo_connect() {
    let mut graph = NodeGraph::new();
    let n1 = graph.add_node("N1", (0.0, 0.0));
    let out = graph.add_port(n1, PortDirection::Output, SocketType::Float, "Out");
    let n2 = graph.add_node("N2", (200.0, 0.0));
    let inp = graph.add_port(n2, PortDirection::Input, SocketType::Float, "In");

    let mut history = CommandHistory::new();
    history.execute(
        Box::new(ConnectCommand::new(out, inp)),
        &mut graph,
    );
    assert_eq!(graph.connection_count(), 1);

    // Undo
    assert!(history.undo(&mut graph));
    assert_eq!(graph.connection_count(), 0);

    // Redo
    assert!(history.redo(&mut graph));
    assert_eq!(graph.connection_count(), 1);

    // Verify endpoints are correct after redo
    let (_, ep) = graph.world.query::<ConnectionEndpoints>().next().unwrap();
    assert_eq!(ep.source_port, out);
    assert_eq!(ep.target_port, inp);
}

// ============================================================
// AC 3: Redo stack clearing
// ============================================================

#[test]
fn redo_stack_clears_on_new_command() {
    let mut graph = NodeGraph::new();
    let node = graph.add_node("N", (0.0, 0.0));
    let mut history = CommandHistory::new();

    // Execute 3 commands
    history.execute(
        Box::new(MoveNodesCommand { node_ids: vec![node], delta_x: 10.0, delta_y: 0.0 }),
        &mut graph,
    );
    history.execute(
        Box::new(MoveNodesCommand { node_ids: vec![node], delta_x: 20.0, delta_y: 0.0 }),
        &mut graph,
    );
    history.execute(
        Box::new(MoveNodesCommand { node_ids: vec![node], delta_x: 30.0, delta_y: 0.0 }),
        &mut graph,
    );
    assert_eq!(history.undo_count(), 3);

    // Undo 1
    history.undo(&mut graph);
    assert_eq!(history.undo_count(), 2);
    assert_eq!(history.redo_count(), 1);
    assert!(history.can_redo());

    // Execute a new command — redo stack must be cleared
    history.execute(
        Box::new(MoveNodesCommand { node_ids: vec![node], delta_x: 5.0, delta_y: 0.0 }),
        &mut graph,
    );
    assert!(!history.can_redo());
    assert_eq!(history.redo_count(), 0);
    assert_eq!(history.undo_count(), 3);
}

// ============================================================
// AC 4: Duplicate
// ============================================================

#[test]
fn duplicate_connected_nodes() {
    let mut graph = NodeGraph::new();
    let (n1, _n1_out, n2, _n2_in, _conn) = make_two_connected_nodes(&mut graph);
    assert_eq!(graph.node_count(), 2);
    assert_eq!(graph.connection_count(), 1);

    let mut history = CommandHistory::new();
    history.execute(
        Box::new(DuplicateNodesCommand::new(vec![n1, n2], (50.0, 50.0))),
        &mut graph,
    );

    assert_eq!(graph.node_count(), 4);
    assert_eq!(graph.connection_count(), 2); // original + duplicated internal

    // Check that new nodes are offset
    let all_positions: Vec<(f64, f64)> = graph.world.query::<NodePosition>()
        .map(|(_, p)| (p.x, p.y))
        .collect();
    assert!(all_positions.contains(&(0.0, 0.0))); // original n1
    assert!(all_positions.contains(&(200.0, 0.0))); // original n2
    assert!(all_positions.contains(&(50.0, 50.0))); // duplicated n1
    assert!(all_positions.contains(&(250.0, 50.0))); // duplicated n2

    // Undo
    history.undo(&mut graph);
    assert_eq!(graph.node_count(), 2);
    assert_eq!(graph.connection_count(), 1);
}

// ============================================================
// AC 5: Mute command
// ============================================================

#[test]
fn mute_command() {
    let mut graph = NodeGraph::new();
    let node = graph.add_node("N", (0.0, 0.0));
    let mut history = CommandHistory::new();

    // No MuteState initially
    assert!(graph.world.get::<MuteState>(node).is_none());

    history.execute(
        Box::new(MuteNodeCommand { node_id: node, muted: true }),
        &mut graph,
    );
    assert_eq!(graph.world.get::<MuteState>(node).unwrap().0, true);

    // Undo
    history.undo(&mut graph);
    assert_eq!(graph.world.get::<MuteState>(node).unwrap().0, false);
}

// ============================================================
// AC 6: Serialization roundtrip (already tested in Phase 1,
// but verify it still works with commands context)
// ============================================================

#[test]
fn serialization_roundtrip_with_commands() {
    let mut graph = NodeGraph::new();
    let mut history = CommandHistory::new();

    // Build graph via commands
    let add_n1 = AddNodeCommand::new("Math", (0.0, 0.0), vec![
        (PortDirection::Input, SocketType::Float, "A".to_string()),
        (PortDirection::Output, SocketType::Float, "Out".to_string()),
    ]);
    history.execute(Box::new(add_n1), &mut graph);

    let add_n2 = AddNodeCommand::new("Output", (200.0, 0.0), vec![
        (PortDirection::Input, SocketType::Float, "In".to_string()),
    ]);
    history.execute(Box::new(add_n2), &mut graph);

    // Get the port IDs for connecting
    let nodes: Vec<EntityId> = graph.world.query::<NodeHeader>().map(|(id, _)| id).collect();
    let n1_ports = graph.node_ports(nodes[0]).to_vec();
    let n2_ports = graph.node_ports(nodes[1]).to_vec();
    let out_port = n1_ports.iter().find(|&&p| {
        graph.world.get::<PortDirection>(p) == Some(&PortDirection::Output)
    }).unwrap();
    let in_port = n2_ports.iter().find(|&&p| {
        graph.world.get::<PortDirection>(p) == Some(&PortDirection::Input)
    }).unwrap();

    history.execute(
        Box::new(ConnectCommand::new(*out_port, *in_port)),
        &mut graph,
    );

    // Serialize and roundtrip
    let serialized = graph.serialize();
    let json = serde_json::to_string(&serialized).unwrap();
    let data: crate::serialization::SerializedGraph = serde_json::from_str(&json).unwrap();
    let restored = NodeGraph::deserialize(&data).unwrap();

    assert_eq!(restored.node_count(), 2);
    assert_eq!(restored.connection_count(), 1);
}

// ============================================================
// AC 7: Clipboard roundtrip
// ============================================================

#[test]
fn clipboard_copy_paste() {
    let mut graph = NodeGraph::new();

    // Build a 5-node graph
    let n1 = graph.add_node("N1", (0.0, 0.0));
    let n1_out = graph.add_port(n1, PortDirection::Output, SocketType::Float, "Out");
    let n2 = graph.add_node("N2", (200.0, 0.0));
    let n2_in = graph.add_port(n2, PortDirection::Input, SocketType::Float, "In");
    let n2_out = graph.add_port(n2, PortDirection::Output, SocketType::Float, "Out");
    let n3 = graph.add_node("N3", (400.0, 0.0));
    let n3_in = graph.add_port(n3, PortDirection::Input, SocketType::Float, "In");
    let _n4 = graph.add_node("N4", (0.0, 200.0));
    let _n5 = graph.add_node("N5", (200.0, 200.0));

    // Connect n1->n2, n2->n3
    graph.connect(n1_out, n2_in).unwrap();
    graph.connect(n2_out, n3_in).unwrap();

    assert_eq!(graph.node_count(), 5);
    assert_eq!(graph.connection_count(), 2);

    // Copy n1 and n2 (one connection between them)
    let clipboard = copy_nodes(&graph, &[n1, n2]);
    assert_eq!(clipboard.nodes.len(), 2);
    assert_eq!(clipboard.connections.len(), 1); // only the n1->n2 connection (n2->n3 is external)

    // Paste with offset
    let new_nodes = paste_nodes(&mut graph, &clipboard, (50.0, 100.0));
    assert_eq!(new_nodes.len(), 2);

    assert_eq!(graph.node_count(), 7);
    // Original 2 connections + 1 pasted internal connection
    assert_eq!(graph.connection_count(), 3);
}

// ============================================================
// Additional tests
// ============================================================

#[test]
fn disconnect_command_undo() {
    let mut graph = NodeGraph::new();
    let (_n1, _n1_out, _n2, _n2_in, conn) = make_two_connected_nodes(&mut graph);
    let mut history = CommandHistory::new();

    history.execute(
        Box::new(DisconnectCommand::new(conn)),
        &mut graph,
    );
    assert_eq!(graph.connection_count(), 0);

    history.undo(&mut graph);
    assert_eq!(graph.connection_count(), 1);
}

#[test]
fn add_node_command_undo() {
    let mut graph = NodeGraph::new();
    let mut history = CommandHistory::new();

    history.execute(
        Box::new(AddNodeCommand::new("Test", (50.0, 50.0), vec![
            (PortDirection::Input, SocketType::Float, "In".to_string()),
        ])),
        &mut graph,
    );
    assert_eq!(graph.node_count(), 1);

    history.undo(&mut graph);
    assert_eq!(graph.node_count(), 0);

    history.redo(&mut graph);
    assert_eq!(graph.node_count(), 1);
}

#[test]
fn collapse_command_undo() {
    let mut graph = NodeGraph::new();
    let node = graph.add_node("N", (0.0, 0.0));
    let mut history = CommandHistory::new();

    assert_eq!(graph.world.get::<NodeHeader>(node).unwrap().collapsed, false);

    history.execute(
        Box::new(CollapseNodeCommand { node_id: node, collapsed: true }),
        &mut graph,
    );
    assert_eq!(graph.world.get::<NodeHeader>(node).unwrap().collapsed, true);

    history.undo(&mut graph);
    assert_eq!(graph.world.get::<NodeHeader>(node).unwrap().collapsed, false);
}

#[test]
fn connect_replaces_existing_and_undo_restores() {
    let mut graph = NodeGraph::new();
    let n1 = graph.add_node("N1", (0.0, 0.0));
    let n1_out = graph.add_port(n1, PortDirection::Output, SocketType::Float, "Out");
    let n2 = graph.add_node("N2", (100.0, 0.0));
    let n2_out = graph.add_port(n2, PortDirection::Output, SocketType::Float, "Out");
    let n3 = graph.add_node("N3", (200.0, 0.0));
    let n3_in = graph.add_port(n3, PortDirection::Input, SocketType::Float, "In");

    // Initial connection: n1 -> n3
    graph.connect(n1_out, n3_in).unwrap();
    assert_eq!(graph.connection_count(), 1);

    let mut history = CommandHistory::new();

    // Command: connect n2 -> n3 (replaces n1 -> n3)
    history.execute(
        Box::new(ConnectCommand::new(n2_out, n3_in)),
        &mut graph,
    );
    assert_eq!(graph.connection_count(), 1);
    let ep = graph.world.query::<ConnectionEndpoints>().next().unwrap().1;
    assert_eq!(ep.source_port, n2_out);

    // Undo should restore n1 -> n3
    history.undo(&mut graph);
    assert_eq!(graph.connection_count(), 1);
    let ep = graph.world.query::<ConnectionEndpoints>().next().unwrap().1;
    assert_eq!(ep.source_port, n1_out);
    assert_eq!(ep.target_port, n3_in);
}

#[test]
fn empty_undo_redo_returns_false() {
    let mut graph = NodeGraph::new();
    let mut history = CommandHistory::new();
    assert!(!history.undo(&mut graph));
    assert!(!history.redo(&mut graph));
    assert!(!history.can_undo());
    assert!(!history.can_redo());
}

// ============================================================
// Blocker fixes: missing edge case tests
// ============================================================

#[test]
fn remove_node_command_undo_restores_connections() {
    let mut graph = NodeGraph::new();
    let n1 = graph.add_node("N1", (0.0, 0.0));
    let n1_out = graph.add_port(n1, PortDirection::Output, SocketType::Float, "Out");

    let n2 = graph.add_node("N2", (200.0, 0.0));
    let n2_in = graph.add_port(n2, PortDirection::Input, SocketType::Float, "In");
    let n2_out = graph.add_port(n2, PortDirection::Output, SocketType::Float, "Out");

    let n3 = graph.add_node("N3", (400.0, 0.0));
    let n3_in = graph.add_port(n3, PortDirection::Input, SocketType::Float, "In");

    // n1 -> n2 -> n3
    graph.connect(n1_out, n2_in).unwrap();
    graph.connect(n2_out, n3_in).unwrap();
    assert_eq!(graph.node_count(), 3);
    assert_eq!(graph.connection_count(), 2);

    let mut history = CommandHistory::new();

    // Remove n2 (middle node — has both incoming and outgoing external connections)
    history.execute(Box::new(RemoveNodeCommand::new(n2)), &mut graph);
    assert_eq!(graph.node_count(), 2);
    assert_eq!(graph.connection_count(), 0); // both connections removed

    // Undo — n2 should be restored with its connections
    history.undo(&mut graph);
    assert_eq!(graph.node_count(), 3);
    assert_eq!(graph.connection_count(), 2);

    // Verify the restored node has correct title
    let titles: Vec<String> = graph.world.query::<NodeHeader>()
        .map(|(_, h)| h.title.clone()).collect();
    assert!(titles.contains(&"N2".to_string()));
}

#[test]
fn duplicate_single_isolated_node() {
    let mut graph = NodeGraph::new();
    let n1 = graph.add_node("Solo", (100.0, 100.0));
    graph.add_port(n1, PortDirection::Input, SocketType::Float, "In");

    let mut history = CommandHistory::new();
    history.execute(
        Box::new(DuplicateNodesCommand::new(vec![n1], (20.0, 20.0))),
        &mut graph,
    );

    assert_eq!(graph.node_count(), 2);
    assert_eq!(graph.connection_count(), 0);

    // Check offset position
    let positions: Vec<(f64, f64)> = graph.world.query::<NodePosition>()
        .map(|(_, p)| (p.x, p.y)).collect();
    assert!(positions.contains(&(100.0, 100.0)));
    assert!(positions.contains(&(120.0, 120.0)));
}

#[test]
fn duplicate_subset_excludes_external_connections() {
    let mut graph = NodeGraph::new();
    let n1 = graph.add_node("N1", (0.0, 0.0));
    let n1_out = graph.add_port(n1, PortDirection::Output, SocketType::Float, "Out");
    let n2 = graph.add_node("N2", (200.0, 0.0));
    let n2_in = graph.add_port(n2, PortDirection::Input, SocketType::Float, "In");
    let n2_out = graph.add_port(n2, PortDirection::Output, SocketType::Float, "Out");
    let n3 = graph.add_node("N3", (400.0, 0.0));
    let n3_in = graph.add_port(n3, PortDirection::Input, SocketType::Float, "In");

    graph.connect(n1_out, n2_in).unwrap();
    graph.connect(n2_out, n3_in).unwrap();

    // Duplicate only n2 — should not duplicate external connections to n1 or n3
    let mut history = CommandHistory::new();
    history.execute(
        Box::new(DuplicateNodesCommand::new(vec![n2], (50.0, 50.0))),
        &mut graph,
    );

    assert_eq!(graph.node_count(), 4);
    // Only original 2 connections — no new connections from the duplicate
    assert_eq!(graph.connection_count(), 2);
}

#[test]
fn copy_empty_selection() {
    let mut graph = NodeGraph::new();
    graph.add_node("N1", (0.0, 0.0));

    let clipboard = copy_nodes(&graph, &[]);
    assert_eq!(clipboard.nodes.len(), 0);
    assert_eq!(clipboard.connections.len(), 0);
}

#[test]
fn paste_into_empty_graph() {
    // Build source graph and copy
    let mut source = NodeGraph::new();
    let n1 = source.add_node("A", (0.0, 0.0));
    let out = source.add_port(n1, PortDirection::Output, SocketType::Float, "Out");
    let n2 = source.add_node("B", (200.0, 0.0));
    let inp = source.add_port(n2, PortDirection::Input, SocketType::Float, "In");
    source.connect(out, inp).unwrap();

    let clipboard = copy_nodes(&source, &[n1, n2]);

    // Paste into fresh empty graph
    let mut target = NodeGraph::new();
    let new_nodes = paste_nodes(&mut target, &clipboard, (10.0, 10.0));

    assert_eq!(new_nodes.len(), 2);
    assert_eq!(target.node_count(), 2);
    assert_eq!(target.connection_count(), 1);

    // Verify positions are offset
    let positions: Vec<(f64, f64)> = target.world.query::<NodePosition>()
        .map(|(_, p)| (p.x, p.y)).collect();
    assert!(positions.contains(&(10.0, 10.0)));
    assert!(positions.contains(&(210.0, 10.0)));
}

#[test]
fn multiple_undo_redo_cycles() {
    let mut graph = NodeGraph::new();
    let node = graph.add_node("N", (0.0, 0.0));
    let mut history = CommandHistory::new();

    // Execute 5 moves: each adds 10 to x
    for _ in 0..5 {
        history.execute(
            Box::new(MoveNodesCommand { node_ids: vec![node], delta_x: 10.0, delta_y: 0.0 }),
            &mut graph,
        );
    }
    assert!((graph.world.get::<NodePosition>(node).unwrap().x - 50.0).abs() < 1e-10);

    // Undo all 5
    for _ in 0..5 {
        assert!(history.undo(&mut graph));
    }
    assert!((graph.world.get::<NodePosition>(node).unwrap().x - 0.0).abs() < 1e-10);
    assert!(!history.can_undo());

    // Redo all 5
    for _ in 0..5 {
        assert!(history.redo(&mut graph));
    }
    assert!((graph.world.get::<NodePosition>(node).unwrap().x - 50.0).abs() < 1e-10);
    assert!(!history.can_redo());
}
