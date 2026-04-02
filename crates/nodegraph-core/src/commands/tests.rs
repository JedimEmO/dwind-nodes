use super::*;
use crate::graph::GraphEditor;
use crate::graph::node::{NodeHeader, NodePosition, MuteState};
use crate::graph::port::PortDirection;
use crate::graph::connection::ConnectionEndpoints;
use crate::types::socket_type::SocketType;
use crate::store::EntityId;

fn make_chain(editor: &mut GraphEditor) -> (EntityId, EntityId, EntityId) {
    let g = editor.current_graph_mut();
    let n1 = g.add_node("A", (0.0, 0.0));
    let n1_out = g.add_port(n1, PortDirection::Output, SocketType::Float, "Out");
    let n2 = g.add_node("B", (200.0, 0.0));
    let n2_in = g.add_port(n2, PortDirection::Input, SocketType::Float, "In");
    let n2_out = g.add_port(n2, PortDirection::Output, SocketType::Float, "Out");
    let n3 = g.add_node("C", (400.0, 0.0));
    let n3_in = g.add_port(n3, PortDirection::Input, SocketType::Float, "In");
    g.connect(n1_out, n2_in).unwrap();
    g.connect(n2_out, n3_in).unwrap();
    (n1, n2, n3)
}

#[test]
fn undo_redo_move() {
    let mut editor = GraphEditor::new();
    let mut history = UndoHistory::new();
    let node = editor.current_graph_mut().add_node("N", (0.0, 0.0));

    // Move
    history.save(&editor);
    editor.current_graph_mut().world.get_mut::<NodePosition>(node).unwrap().x = 100.0;
    editor.current_graph_mut().world.get_mut::<NodePosition>(node).unwrap().y = 50.0;

    let pos = editor.current_graph().world.get::<NodePosition>(node).unwrap();
    assert!((pos.x - 100.0).abs() < 1e-10);

    // Undo
    assert!(history.undo(&mut editor));
    let pos = editor.current_graph().world.get::<NodePosition>(node).unwrap();
    assert!((pos.x - 0.0).abs() < 1e-10);

    // Redo
    assert!(history.redo(&mut editor));
    let pos = editor.current_graph().world.get::<NodePosition>(node).unwrap();
    assert!((pos.x - 100.0).abs() < 1e-10);
}

#[test]
fn undo_redo_connect() {
    let mut editor = GraphEditor::new();
    let mut history = UndoHistory::new();
    let g = editor.current_graph_mut();
    let n1 = g.add_node("N1", (0.0, 0.0));
    let out = g.add_port(n1, PortDirection::Output, SocketType::Float, "Out");
    let n2 = g.add_node("N2", (200.0, 0.0));
    let inp = g.add_port(n2, PortDirection::Input, SocketType::Float, "In");

    history.save(&editor);
    editor.current_graph_mut().connect(out, inp).unwrap();
    assert_eq!(editor.current_graph().connection_count(), 1);

    history.undo(&mut editor);
    assert_eq!(editor.current_graph().connection_count(), 0);

    history.redo(&mut editor);
    assert_eq!(editor.current_graph().connection_count(), 1);
}

#[test]
fn redo_stack_clears_on_new_action() {
    let mut editor = GraphEditor::new();
    let mut history = UndoHistory::new();
    let node = editor.current_graph_mut().add_node("N", (0.0, 0.0));

    history.save(&editor);
    editor.current_graph_mut().world.get_mut::<NodePosition>(node).unwrap().x = 10.0;

    history.save(&editor);
    editor.current_graph_mut().world.get_mut::<NodePosition>(node).unwrap().x = 20.0;

    history.undo(&mut editor);
    assert!(history.can_redo());

    // New action clears redo
    history.save(&editor);
    editor.current_graph_mut().world.get_mut::<NodePosition>(node).unwrap().x = 5.0;
    assert!(!history.can_redo());
}

#[test]
fn undo_group() {
    let mut editor = GraphEditor::new();
    let mut history = UndoHistory::new();
    let (_n1, n2, _n3) = make_chain(&mut editor);

    assert_eq!(editor.current_graph().node_count(), 3);
    assert_eq!(editor.current_graph().connection_count(), 2);

    // Group B
    history.save(&editor);
    editor.group_nodes(&[n2]);

    assert_eq!(editor.current_graph().node_count(), 3); // A, C, Group

    // Undo
    history.undo(&mut editor);
    assert_eq!(editor.current_graph().node_count(), 3); // A, B, C
    assert_eq!(editor.current_graph().connection_count(), 2);

    // Redo
    history.redo(&mut editor);
    assert_eq!(editor.current_graph().node_count(), 3); // A, C, Group
    let has_group = editor.current_graph().world.query::<crate::graph::group::SubgraphRoot>().count();
    assert_eq!(has_group, 1);

    // Undo again
    history.undo(&mut editor);
    assert_eq!(editor.current_graph().node_count(), 3);
    assert_eq!(editor.current_graph().connection_count(), 2);
}

#[test]
fn undo_redo_group_many_cycles() {
    let mut editor = GraphEditor::new();
    let mut history = UndoHistory::new();
    let (_n1, n2, _n3) = make_chain(&mut editor);

    for _ in 0..5 {
        history.save(&editor);
        editor.group_nodes(&[n2]);
        history.undo(&mut editor);
        assert_eq!(editor.current_graph().node_count(), 3);
        assert_eq!(editor.current_graph().connection_count(), 2);
        history.redo(&mut editor);
        history.undo(&mut editor);
    }

    assert_eq!(editor.current_graph().node_count(), 3);
    assert_eq!(editor.current_graph().connection_count(), 2);
}

#[test]
fn undo_ungroup() {
    let mut editor = GraphEditor::new();
    let mut history = UndoHistory::new();
    let (_n1, n2, _n3) = make_chain(&mut editor);

    editor.group_nodes(&[n2]);
    let group_node = editor.current_graph().world
        .query::<crate::graph::group::SubgraphRoot>()
        .map(|(id, _)| id).next().unwrap();

    history.save(&editor);
    editor.ungroup(group_node);
    assert_eq!(editor.current_graph().node_count(), 3); // A, B, C restored

    history.undo(&mut editor);
    assert_eq!(editor.current_graph().node_count(), 3); // A, C, Group
    let has_group = editor.current_graph().world.query::<crate::graph::group::SubgraphRoot>().count();
    assert_eq!(has_group, 1);
}

#[test]
fn clipboard_roundtrip() {
    let mut editor = GraphEditor::new();
    let (n1, n2, _n3) = make_chain(&mut editor);

    let clipboard = copy_nodes(editor.current_graph(), &[n1, n2]);
    assert_eq!(clipboard.nodes.len(), 2);
    assert_eq!(clipboard.connections.len(), 1);

    let new_nodes = paste_nodes(editor.current_graph_mut(), &clipboard, (50.0, 100.0));
    assert_eq!(new_nodes.len(), 2);
    assert_eq!(editor.current_graph().node_count(), 5); // 3 original + 2 pasted
}

#[test]
fn empty_undo_redo() {
    let mut editor = GraphEditor::new();
    let mut history = UndoHistory::new();
    assert!(!history.undo(&mut editor));
    assert!(!history.redo(&mut editor));
}
