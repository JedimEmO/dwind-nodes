use super::*;
use crate::graph::NodeGraph;
use crate::graph::node::{NodeHeader, NodePosition};
use crate::graph::port::{PortDirection, PortSocketType};
use crate::graph::connection::ConnectionEndpoints;
use crate::types::socket_type::SocketType;

// ============================================================
// Acceptance Criterion 1: Entity lifecycle
// ============================================================

#[test]
fn entity_lifecycle_spawn_despawn_reuse() {
    let mut world = World::new();

    let mut ids: Vec<EntityId> = Vec::new();
    for _ in 0..1000 {
        ids.push(world.spawn());
    }
    assert_eq!(world.entity_count(), 1000);

    let mut despawned = Vec::new();
    for i in (0..1000).step_by(2) {
        assert!(world.despawn(ids[i]));
        despawned.push(ids[i]);
    }
    assert_eq!(world.entity_count(), 500);

    for &id in &despawned {
        assert!(!world.is_alive(id));
    }

    let mut new_ids = Vec::new();
    for _ in 0..500 {
        new_ids.push(world.spawn());
    }
    assert_eq!(world.entity_count(), 1000);

    for &id in &despawned {
        assert!(!world.is_alive(id));
    }
    for &id in &new_ids {
        assert!(world.is_alive(id));
    }
}

#[test]
fn entity_despawned_component_inaccessible() {
    let mut world = World::new();
    let id = world.spawn();
    world.insert(id, 42u32);
    assert_eq!(world.get::<u32>(id), Some(&42));

    world.despawn(id);
    assert_eq!(world.get::<u32>(id), None);
}

#[test]
fn entity_generation_prevents_stale_access() {
    let mut world = World::new();
    let old_id = world.spawn();
    world.insert(old_id, 100u32);
    world.despawn(old_id);

    let new_id = world.spawn();
    world.insert(new_id, 200u32);

    assert_eq!(world.get::<u32>(old_id), None);
    assert_eq!(world.get::<u32>(new_id), Some(&200));
    assert_ne!(old_id, new_id);
}

#[test]
fn entity_double_despawn_returns_false() {
    let mut world = World::new();
    let id = world.spawn();
    assert!(world.despawn(id));
    assert!(!world.despawn(id));
}

// ============================================================
// Acceptance Criterion 2: Component CRUD
// ============================================================

#[test]
fn component_crud() {
    let mut world = World::new();
    let id = world.spawn();

    world.insert(id, 42u32);
    world.insert(id, 3.14f64);
    world.insert(id, "hello".to_string());

    assert_eq!(world.get::<u32>(id), Some(&42));
    assert_eq!(world.get::<f64>(id), Some(&3.14));
    assert_eq!(world.get::<String>(id), Some(&"hello".to_string()));

    let removed = world.remove::<f64>(id);
    assert_eq!(removed, Some(3.14));

    assert_eq!(world.get::<f64>(id), None);
    assert_eq!(world.get::<u32>(id), Some(&42));
    assert_eq!(world.get::<String>(id), Some(&"hello".to_string()));
}

#[test]
fn component_overwrite() {
    let mut world = World::new();
    let id = world.spawn();
    world.insert(id, 10u32);
    world.insert(id, 20u32);
    assert_eq!(world.get::<u32>(id), Some(&20));
}

#[test]
fn component_get_mut() {
    let mut world = World::new();
    let id = world.spawn();
    world.insert(id, 5u32);
    if let Some(val) = world.get_mut::<u32>(id) {
        *val = 10;
    }
    assert_eq!(world.get::<u32>(id), Some(&10));
}

#[test]
fn component_remove_never_inserted_returns_none() {
    let mut world = World::new();
    let id = world.spawn();
    assert_eq!(world.remove::<u32>(id), None);
    assert_eq!(world.get::<u32>(id), None);
}

#[test]
fn component_has() {
    let mut world = World::new();
    let id = world.spawn();
    assert!(!world.has::<u32>(id));
    world.insert(id, 42u32);
    assert!(world.has::<u32>(id));
}

// ============================================================
// Acceptance Criterion 3: Query correctness
// ============================================================

#[derive(Debug, PartialEq, Clone)]
struct CompA(u32);
#[derive(Debug, PartialEq, Clone)]
struct CompB(u32);

#[test]
fn query_single_component() {
    let mut world = World::new();

    let mut all_ids = Vec::new();
    for _ in 0..100u32 {
        all_ids.push(world.spawn());
    }

    for i in 0..50 {
        world.insert(all_ids[i], CompA(i as u32));
    }

    for i in 20..50 {
        world.insert(all_ids[i], CompB(i as u32));
    }

    assert_eq!(world.query::<CompA>().count(), 50);
    assert_eq!(world.query::<CompB>().count(), 30);
    assert_eq!(world.query2::<CompA, CompB>().count(), 30);
}

#[test]
fn query_returns_correct_data() {
    let mut world = World::new();
    let id1 = world.spawn();
    let id2 = world.spawn();

    world.insert(id1, CompA(1));
    world.insert(id1, CompB(10));
    world.insert(id2, CompA(2));

    let results: Vec<_> = world.query2::<CompA, CompB>().collect();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, id1);
    assert_eq!(results[0].1, &CompA(1));
    assert_eq!(results[0].2, &CompB(10));
}

#[test]
fn query_skips_despawned_entities() {
    let mut world = World::new();
    let id1 = world.spawn();
    let id2 = world.spawn();
    let id3 = world.spawn();

    world.insert(id1, CompA(1));
    world.insert(id2, CompA(2));
    world.insert(id3, CompA(3));

    world.despawn(id2);

    let results: Vec<_> = world.query::<CompA>().map(|(_, a)| a.0).collect();
    assert_eq!(results.len(), 2);
    assert!(results.contains(&1));
    assert!(results.contains(&3));
    assert!(!results.contains(&2));
}

#[test]
fn query_empty_world() {
    let world = World::new();
    assert_eq!(world.query::<CompA>().count(), 0);
    assert_eq!(world.query2::<CompA, CompB>().count(), 0);
}

// ============================================================
// Acceptance Criterion 4: Graph construction
// ============================================================

#[test]
fn graph_construction_three_nodes() {
    let mut graph = NodeGraph::new();

    let node1 = graph.add_node("Node 1", (0.0, 0.0));
    let _node1_in1 = graph.add_port(node1, PortDirection::Input, SocketType::Float, "In 1");
    let _node1_in2 = graph.add_port(node1, PortDirection::Input, SocketType::Float, "In 2");
    let node1_out = graph.add_port(node1, PortDirection::Output, SocketType::Float, "Out");

    let node2 = graph.add_node("Node 2", (200.0, 0.0));
    let node2_in1 = graph.add_port(node2, PortDirection::Input, SocketType::Float, "In 1");
    let _node2_in2 = graph.add_port(node2, PortDirection::Input, SocketType::Float, "In 2");
    let node2_out = graph.add_port(node2, PortDirection::Output, SocketType::Float, "Out");

    let node3 = graph.add_node("Node 3", (400.0, 0.0));
    let node3_in1 = graph.add_port(node3, PortDirection::Input, SocketType::Float, "In 1");
    let _node3_in2 = graph.add_port(node3, PortDirection::Input, SocketType::Float, "In 2");
    let node3_out = graph.add_port(node3, PortDirection::Output, SocketType::Float, "Out");

    graph.connect(node1_out, node2_in1).unwrap();
    graph.connect(node2_out, node3_in1).unwrap();

    assert_eq!(graph.connection_count(), 2);
    assert_eq!(graph.node_count(), 3);

    assert_eq!(graph.node_ports(node1).len(), 3);
    assert_eq!(graph.node_ports(node2).len(), 3);
    assert_eq!(graph.node_ports(node3).len(), 3);

    assert_eq!(graph.port_connections(node1_out).len(), 1);
    assert_eq!(graph.port_connections(node2_in1).len(), 1);
    assert_eq!(graph.port_connections(node2_out).len(), 1);
    assert_eq!(graph.port_connections(node3_in1).len(), 1);

    assert_eq!(graph.port_connections(_node1_in1).len(), 0);
    assert_eq!(graph.port_connections(node3_out).len(), 0);
}

#[test]
fn graph_remove_node_cleans_connections() {
    let mut graph = NodeGraph::new();

    let node1 = graph.add_node("Node 1", (0.0, 0.0));
    let node1_out = graph.add_port(node1, PortDirection::Output, SocketType::Float, "Out");

    let node2 = graph.add_node("Node 2", (200.0, 0.0));
    let node2_in = graph.add_port(node2, PortDirection::Input, SocketType::Float, "In");

    graph.connect(node1_out, node2_in).unwrap();
    assert_eq!(graph.connection_count(), 1);

    graph.remove_node(node1);
    assert_eq!(graph.connection_count(), 0);
    assert_eq!(graph.node_count(), 1);
    assert_eq!(graph.port_connections(node2_in).len(), 0);
}

// ============================================================
// Acceptance Criterion 5: Connection validation
// ============================================================

#[test]
fn connection_validation_same_direction_fails() {
    let mut graph = NodeGraph::new();
    let node1 = graph.add_node("N1", (0.0, 0.0));
    let node2 = graph.add_node("N2", (100.0, 0.0));
    let in1 = graph.add_port(node1, PortDirection::Input, SocketType::Float, "In");
    let in2 = graph.add_port(node2, PortDirection::Input, SocketType::Float, "In");

    let result = graph.connect(in1, in2);
    assert!(matches!(result, Err(crate::graph::ConnectionError::SameDirection)));
}

#[test]
fn connection_validation_incompatible_types_fails() {
    let mut graph = NodeGraph::new();
    let node1 = graph.add_node("N1", (0.0, 0.0));
    let node2 = graph.add_node("N2", (100.0, 0.0));
    let out = graph.add_port(node1, PortDirection::Output, SocketType::Float, "Out");
    let inp = graph.add_port(node2, PortDirection::Input, SocketType::Shader, "In");

    let result = graph.connect(out, inp);
    assert!(matches!(result, Err(crate::graph::ConnectionError::IncompatibleTypes(_, _))));
}

#[test]
fn connection_validation_compatible_succeeds() {
    let mut graph = NodeGraph::new();
    let node1 = graph.add_node("N1", (0.0, 0.0));
    let node2 = graph.add_node("N2", (100.0, 0.0));
    let out = graph.add_port(node1, PortDirection::Output, SocketType::Float, "Out");
    let inp = graph.add_port(node2, PortDirection::Input, SocketType::Float, "In");

    assert!(graph.connect(out, inp).is_ok());
}

#[test]
fn connection_validation_implicit_conversion() {
    let mut graph = NodeGraph::new();
    let node1 = graph.add_node("N1", (0.0, 0.0));
    let node2 = graph.add_node("N2", (100.0, 0.0));
    let out = graph.add_port(node1, PortDirection::Output, SocketType::Float, "Out");
    let inp = graph.add_port(node2, PortDirection::Input, SocketType::Int, "In");

    assert!(graph.connect(out, inp).is_ok());
}

#[test]
fn connection_validation_same_node_fails() {
    let mut graph = NodeGraph::new();
    let node = graph.add_node("N", (0.0, 0.0));
    let out = graph.add_port(node, PortDirection::Output, SocketType::Float, "Out");
    let inp = graph.add_port(node, PortDirection::Input, SocketType::Float, "In");

    let result = graph.connect(out, inp);
    assert!(matches!(result, Err(crate::graph::ConnectionError::SameNode)));
}

#[test]
fn connection_replaces_existing_on_input() {
    let mut graph = NodeGraph::new();
    let node1 = graph.add_node("N1", (0.0, 0.0));
    let node2 = graph.add_node("N2", (100.0, 0.0));
    let node3 = graph.add_node("N3", (200.0, 0.0));

    let out1 = graph.add_port(node1, PortDirection::Output, SocketType::Float, "Out");
    let out2 = graph.add_port(node2, PortDirection::Output, SocketType::Float, "Out");
    let inp = graph.add_port(node3, PortDirection::Input, SocketType::Float, "In");

    graph.connect(out1, inp).unwrap();
    assert_eq!(graph.connection_count(), 1);

    graph.connect(out2, inp).unwrap();
    assert_eq!(graph.connection_count(), 1);
    assert_eq!(graph.port_connections(inp).len(), 1);

    let conn_id = graph.port_connections(inp)[0];
    let endpoints = graph.world.get::<ConnectionEndpoints>(conn_id).unwrap();
    assert_eq!(endpoints.source_port, out2);
    assert_eq!(endpoints.target_port, inp);
}

#[test]
fn connection_to_invalid_port_fails() {
    let mut graph = NodeGraph::new();
    let node1 = graph.add_node("N1", (0.0, 0.0));
    let out = graph.add_port(node1, PortDirection::Output, SocketType::Float, "Out");

    // Fabricate a non-existent port ID
    let fake_port = EntityId { index: 9999, generation: crate::store::Generation::default() };
    let result = graph.connect(out, fake_port);
    assert!(matches!(result, Err(crate::graph::ConnectionError::InvalidTargetPort)));
}

// ============================================================
// Acceptance Criterion 6: Serialization roundtrip
// ============================================================

#[test]
fn serialization_roundtrip() {
    let mut graph = NodeGraph::new();

    let node1 = graph.add_node("Math Add", (10.0, 20.0));
    let _n1_in1 = graph.add_port(node1, PortDirection::Input, SocketType::Float, "A");
    let _n1_in2 = graph.add_port(node1, PortDirection::Input, SocketType::Float, "B");
    let n1_out = graph.add_port(node1, PortDirection::Output, SocketType::Float, "Result");

    let node2 = graph.add_node("Color Mix", (200.0, 100.0));
    let _n2_in1 = graph.add_port(node2, PortDirection::Input, SocketType::Color, "Color 1");
    let _n2_in2 = graph.add_port(node2, PortDirection::Input, SocketType::Color, "Color 2");
    let n2_fac = graph.add_port(node2, PortDirection::Input, SocketType::Float, "Factor");
    let n2_out = graph.add_port(node2, PortDirection::Output, SocketType::Color, "Color");

    let node3 = graph.add_node("Output", (400.0, 50.0));
    let n3_in = graph.add_port(node3, PortDirection::Input, SocketType::Color, "Surface");

    graph.connect(n1_out, n2_fac).unwrap();
    graph.connect(n2_out, n3_in).unwrap();

    // Serialize
    let serialized = graph.serialize();
    let json = serde_json::to_string_pretty(&serialized).unwrap();

    // Deserialize
    let deserialized_data: crate::serialization::SerializedGraph = serde_json::from_str(&json).unwrap();
    let restored = NodeGraph::deserialize(&deserialized_data).unwrap();

    // Verify structure
    assert_eq!(restored.node_count(), 3);
    assert_eq!(restored.connection_count(), 2);

    // Verify node titles
    let headers: Vec<_> = restored.world.query::<NodeHeader>().map(|(_, h)| h.title.clone()).collect();
    assert!(headers.contains(&"Math Add".to_string()));
    assert!(headers.contains(&"Color Mix".to_string()));
    assert!(headers.contains(&"Output".to_string()));

    // Verify node positions survived roundtrip
    for (id, header) in restored.world.query::<NodeHeader>() {
        let pos = restored.world.get::<NodePosition>(id).unwrap();
        match header.title.as_str() {
            "Math Add" => { assert_eq!(pos.x, 10.0); assert_eq!(pos.y, 20.0); }
            "Color Mix" => { assert_eq!(pos.x, 200.0); assert_eq!(pos.y, 100.0); }
            "Output" => { assert_eq!(pos.x, 400.0); assert_eq!(pos.y, 50.0); }
            _ => panic!("unexpected node"),
        }
    }

    // Verify connection endpoints reference correct port types
    for (_, endpoints) in restored.world.query::<ConnectionEndpoints>() {
        let src_type = restored.world.get::<PortSocketType>(endpoints.source_port).unwrap();
        let tgt_type = restored.world.get::<PortSocketType>(endpoints.target_port).unwrap();
        // All connections in this graph go from Float output or Color output
        assert!(src_type.0.is_compatible_with(&tgt_type.0));
    }

    // Verify port directions are correct on connection endpoints
    for (_, endpoints) in restored.world.query::<ConnectionEndpoints>() {
        let src_dir = restored.world.get::<PortDirection>(endpoints.source_port).unwrap();
        let tgt_dir = restored.world.get::<PortDirection>(endpoints.target_port).unwrap();
        assert_eq!(*src_dir, PortDirection::Output);
        assert_eq!(*tgt_dir, PortDirection::Input);
    }
}

#[test]
fn serialization_empty_graph() {
    let graph = NodeGraph::new();
    let serialized = graph.serialize();
    let json = serde_json::to_string(&serialized).unwrap();
    let deserialized: crate::serialization::SerializedGraph = serde_json::from_str(&json).unwrap();
    let restored = NodeGraph::deserialize(&deserialized).unwrap();
    assert_eq!(restored.node_count(), 0);
    assert_eq!(restored.connection_count(), 0);
}

#[test]
fn deserialization_orphaned_connection_returns_error() {
    let data = crate::serialization::SerializedGraph {
        nodes: vec![],
        connections: vec![crate::serialization::SerializedConnection {
            id: 0,
            source_port: 999,
            target_port: 888,
        }],
        frames: vec![],
    };
    let result = NodeGraph::deserialize(&data);
    assert!(matches!(result, Err(crate::serialization::DeserializeError::OrphanedConnection { .. })));
}

#[test]
fn serialization_roundtrip_frames_and_reroutes() {
    use crate::graph::frame::{FrameRect, FrameLabel, FrameMembers};
    use crate::graph::reroute::IsReroute;

    let mut graph = NodeGraph::new();
    let n1 = graph.add_node("A", (0.0, 0.0));
    graph.add_port(n1, PortDirection::Output, SocketType::Float, "Out");
    let n2 = graph.add_node("B", (200.0, 0.0));
    graph.add_port(n2, PortDirection::Input, SocketType::Float, "In");

    // Add a reroute
    let reroute = graph.add_node("Reroute", (100.0, 0.0));
    graph.add_port(reroute, PortDirection::Input, SocketType::Any, "");
    graph.add_port(reroute, PortDirection::Output, SocketType::Any, "");
    graph.world.insert(reroute, IsReroute);

    // Add a frame around n1 and n2
    graph.add_frame("My Frame", [255, 100, 50], &[n1, n2]);

    // Roundtrip
    let serialized = graph.serialize();
    let json = serde_json::to_string(&serialized).unwrap();
    let data: crate::serialization::SerializedGraph = serde_json::from_str(&json).unwrap();
    let restored = NodeGraph::deserialize(&data).unwrap();

    // Verify reroute marker survived
    let reroute_count = restored.world.query::<IsReroute>().count();
    assert_eq!(reroute_count, 1, "IsReroute should survive serialization roundtrip");

    // Verify frame survived
    assert_eq!(restored.frame_count(), 1, "Frame should survive serialization roundtrip");
    let (fid, _) = restored.world.query::<FrameRect>().next().unwrap();
    let label = restored.world.get::<FrameLabel>(fid).unwrap();
    assert_eq!(label.0, "My Frame");
    let members = restored.world.get::<FrameMembers>(fid).unwrap();
    assert_eq!(members.0.len(), 2, "Frame should have 2 members after roundtrip");
}

#[test]
fn serialization_roundtrip_with_subgraphs() {
    use crate::graph::GraphEditor;

    let mut ge = GraphEditor::new();

    // Build A → B → C chain in root
    let a = ge.current_graph_mut().add_node("A", (0.0, 0.0));
    let a_out = ge.current_graph_mut().add_port(a, PortDirection::Output, SocketType::Float, "Out");
    let b = ge.current_graph_mut().add_node("B", (200.0, 0.0));
    let b_in = ge.current_graph_mut().add_port(b, PortDirection::Input, SocketType::Float, "In");
    let b_out = ge.current_graph_mut().add_port(b, PortDirection::Output, SocketType::Float, "Out");
    let c = ge.current_graph_mut().add_node("C", (400.0, 0.0));
    let c_in = ge.current_graph_mut().add_port(c, PortDirection::Input, SocketType::Float, "In");
    ge.current_graph_mut().connect(a_out, b_in).unwrap();
    ge.current_graph_mut().connect(b_out, c_in).unwrap();

    // Group B
    let (group_node, _subgraph_id, _) = ge.group_nodes(&[b]).unwrap();

    // Serialize root graph
    let root_serialized = ge.current_graph().serialize();
    let root_json = serde_json::to_string(&root_serialized).unwrap();
    let root_data: crate::serialization::SerializedGraph = serde_json::from_str(&root_json).unwrap();
    let root_restored = NodeGraph::deserialize(&root_data).unwrap();

    // Root should have A, C, Group (3 nodes), 2 connections
    assert_eq!(root_restored.node_count(), 3, "Root roundtrip: 3 nodes");
    assert_eq!(root_restored.connection_count(), 2, "Root roundtrip: 2 connections");

    // Serialize subgraph
    ge.enter_group(group_node);
    let sub_serialized = ge.current_graph().serialize();
    let sub_json = serde_json::to_string(&sub_serialized).unwrap();
    let sub_data: crate::serialization::SerializedGraph = serde_json::from_str(&sub_json).unwrap();
    let sub_restored = NodeGraph::deserialize(&sub_data).unwrap();

    // Subgraph should have IO nodes + B, with connections
    assert!(sub_restored.node_count() >= 2, "Subgraph roundtrip: at least B + IO nodes, got {}", sub_restored.node_count());
    assert!(sub_restored.connection_count() >= 1, "Subgraph roundtrip: at least 1 connection, got {}", sub_restored.connection_count());

    // Verify node titles survived
    let titles: Vec<String> = sub_restored.world.query::<NodeHeader>()
        .map(|(_, h)| h.title.clone()).collect();
    assert!(titles.iter().any(|t| t == "B"), "Subgraph should contain node B after roundtrip, got {:?}", titles);
}

#[test]
fn graph_editor_full_roundtrip() {
    use crate::graph::GraphEditor;
    use crate::graph::GroupIOKind;
    use crate::graph::group::SubgraphRoot;

    let mut ge = GraphEditor::new();

    // Build A → B → C chain
    let a = ge.current_graph_mut().add_node("A", (0.0, 0.0));
    let a_out = ge.current_graph_mut().add_port(a, PortDirection::Output, SocketType::Float, "Out");
    let b = ge.current_graph_mut().add_node("B", (200.0, 0.0));
    let b_in = ge.current_graph_mut().add_port(b, PortDirection::Input, SocketType::Float, "In");
    let b_out = ge.current_graph_mut().add_port(b, PortDirection::Output, SocketType::Float, "Out");
    let c = ge.current_graph_mut().add_node("C", (400.0, 0.0));
    let c_in = ge.current_graph_mut().add_port(c, PortDirection::Input, SocketType::Float, "In");
    ge.current_graph_mut().connect(a_out, b_in).unwrap();
    ge.current_graph_mut().connect(b_out, c_in).unwrap();

    // Group B
    let (_group_node, _subgraph_id, _) = ge.group_nodes(&[b]).unwrap();

    // Serialize entire editor
    let serialized = ge.serialize_editor();
    let json = serde_json::to_string_pretty(&serialized).unwrap();
    let data: crate::serialization::SerializedGraphEditor = serde_json::from_str(&json).unwrap();

    // Deserialize
    let restored = GraphEditor::deserialize_editor(&data).unwrap();

    // Root graph should have A, C, Group (3 nodes), 2 connections
    assert_eq!(restored.current_graph().node_count(), 3, "Root: 3 nodes");
    assert_eq!(restored.current_graph().connection_count(), 2, "Root: 2 connections");

    // Should have a group node with SubgraphRoot
    let group_count = restored.current_graph().world.query::<SubgraphRoot>().count();
    assert_eq!(group_count, 1, "Root should have 1 group node with SubgraphRoot");

    // Should have 2 graphs total (root + subgraph)
    assert_eq!(data.graphs.len(), 2, "Should serialize 2 graphs");

    // Enter the group and verify subgraph
    let group_node_id = restored.current_graph().world.query::<SubgraphRoot>()
        .map(|(id, _)| id).next().unwrap();
    let mut restored = restored;
    assert!(restored.enter_group(group_node_id), "Should be able to enter group");

    // Subgraph should have B + IO nodes
    assert!(restored.current_graph().node_count() >= 2, "Subgraph should have B + IO nodes");

    // IO nodes should have GroupIOKind
    let io_count = restored.current_graph().world.query::<GroupIOKind>().count();
    assert!(io_count >= 1, "Subgraph should have IO nodes with GroupIOKind, got {}", io_count);
}

// ============================================================
// Acceptance Criterion 7: Change tracking
// ============================================================

#[test]
fn change_tracking() {
    let mut world = World::new();

    let mut ids = Vec::new();
    for i in 0..10 {
        let id = world.spawn();
        world.insert(id, NodePosition { x: i as f64 * 10.0, y: 0.0 });
        ids.push(id);
    }

    world.change_tracker.clear();
    assert!(!world.change_tracker.has_changes());

    let modified_indices = [0, 2, 4, 6, 8];
    for &i in &modified_indices {
        if let Some(pos) = world.get_mut::<NodePosition>(ids[i]) {
            pos.x += 100.0;
        }
    }

    let changed: Vec<EntityId> = world.change_tracker.changed_entities::<NodePosition>().collect();
    assert_eq!(changed.len(), 5);

    for &i in &modified_indices {
        assert!(changed.contains(&ids[i]));
    }

    for &i in &[1, 3, 5, 7, 9] {
        assert!(!changed.contains(&ids[i]));
    }
}

// ============================================================
// SocketType compatibility tests
// ============================================================

#[test]
fn socket_type_same_type_compatible() {
    assert!(SocketType::Float.is_compatible_with(&SocketType::Float));
    assert!(SocketType::Shader.is_compatible_with(&SocketType::Shader));
    assert!(SocketType::Custom(42).is_compatible_with(&SocketType::Custom(42)));
}

#[test]
fn socket_type_numeric_conversions() {
    assert!(SocketType::Float.is_compatible_with(&SocketType::Int));
    assert!(SocketType::Int.is_compatible_with(&SocketType::Float));
    assert!(SocketType::Float.is_compatible_with(&SocketType::Bool));
    assert!(SocketType::Bool.is_compatible_with(&SocketType::Float));
    assert!(SocketType::Int.is_compatible_with(&SocketType::Bool));
    assert!(SocketType::Bool.is_compatible_with(&SocketType::Int));
}

#[test]
fn socket_type_float_color_vector_conversions() {
    assert!(SocketType::Float.is_compatible_with(&SocketType::Color));
    assert!(SocketType::Color.is_compatible_with(&SocketType::Float));
    assert!(SocketType::Float.is_compatible_with(&SocketType::Vector));
    assert!(SocketType::Vector.is_compatible_with(&SocketType::Float));
}

#[test]
fn socket_type_incompatible() {
    assert!(!SocketType::Float.is_compatible_with(&SocketType::Shader));
    assert!(!SocketType::Shader.is_compatible_with(&SocketType::Geometry));
    assert!(!SocketType::Custom(1).is_compatible_with(&SocketType::Custom(2)));
    assert!(!SocketType::String.is_compatible_with(&SocketType::Float));
    assert!(!SocketType::Vector.is_compatible_with(&SocketType::Color));
    assert!(!SocketType::Image.is_compatible_with(&SocketType::Object));
}
